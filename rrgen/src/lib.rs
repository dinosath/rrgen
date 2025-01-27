#[cfg(all(feature = "tera", feature = "minijinja"))]
compile_error!("You cannot enable both 'tera' and 'minijinja' at the same time.");

#[cfg(not(any(feature = "tera", feature = "minijinja")))]
compile_error!("You must enable exactly one feature: 'tera' or 'minijinja'.");

use std::path::{Path, PathBuf};
use crate::MatchPositions::{All, First, Last};
use regex::Regex;
use serde::Deserialize;
#[cfg(feature = "tera")]
use tera::{Context, Tera};
#[cfg(feature = "minijinja")]
use minijinja::{Environment};
use log::debug;

#[cfg(feature = "tera")]
mod tera_filters;
#[cfg(feature = "minijinja")]
mod minijinja_filters;


pub trait FsDriver {
    /// Write a file
    ///
    /// # Errors
    ///
    /// This function will return an error if it fails
    fn write_file(&self, path: &Path, content: &str) -> Result<()>;

    /// Read a file
    ///
    /// # Errors
    ///
    /// This function will return an error if it fails
    fn read_file(&self, path: &Path) -> Result<String>;

    fn exists(&self, path: &Path) -> bool;
}

pub struct RealFsDriver {}
impl FsDriver for RealFsDriver {
    fn write_file(&self, path: &Path, content: &str) -> Result<()> {
        let dir = path.parent().expect("cannot get folder");
        if !dir.exists() {
            fs_err::create_dir_all(dir)?;
        }
        Ok(fs_err::write(path, content)?)
    }

    fn read_file(&self, path: &Path) -> Result<String> {
        Ok(fs_err::read_to_string(path)?)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
}

pub trait Printer {
    fn overwrite_file(&self, file_to: &Path);
    fn skip_exists(&self, file_to: &Path);
    fn add_file(&self, file_to: &Path);
    fn injected(&self, file_to: &Path);
}
pub struct ConsolePrinter {}
impl Printer for ConsolePrinter {
    fn overwrite_file(&self, file_to: &Path) {
        println!("overwritten: {file_to:?}");
    }

    fn add_file(&self, file_to: &Path) {
        println!("added: {file_to:?}");
    }

    fn injected(&self, file_to: &Path) {
        println!("injected: {file_to:?}");
    }

    fn skip_exists(&self, file_to: &Path) {
        println!("skipped (exists): {file_to:?}");
    }
}

#[derive(Deserialize, Debug, Default, Clone)]
struct FrontMatter {
    #[serde(default)]
    to: Option<String>,

    #[serde(default)]
    skip_exists: bool,

    #[serde(default)]
    skip_glob: Option<String>,

    #[serde(default)]
    message: Option<String>,

    #[serde(default)]
    injections: Option<Vec<Injection>>,
}

#[derive(Deserialize, Debug, Default, Clone)]
struct Injection {
    into: String,
    content: String,

    #[serde(default)]
    inline: bool,

    #[serde(with = "serde_regex")]
    #[serde(default)]
    skip_if: Option<Regex>,

    #[serde(with = "serde_regex")]
    #[serde(default)]
    before: Option<Regex>,

    #[serde(with = "serde_regex")]
    #[serde(default)]
    before_all: Option<Regex>,

    #[serde(with = "serde_regex")]
    #[serde(default)]
    before_last: Option<Regex>,

    #[serde(with = "serde_regex")]
    #[serde(default)]
    after: Option<Regex>,

    #[serde(with = "serde_regex")]
    #[serde(default)]
    after_all: Option<Regex>,

    #[serde(with = "serde_regex")]
    #[serde(default)]
    after_last: Option<Regex>,

    #[serde(with = "serde_regex")]
    #[serde(default)]
    remove_lines: Option<Regex>,

    #[serde(with = "serde_regex")]
    #[serde(default)]
    replace: Option<Regex>,

    #[serde(with = "serde_regex")]
    #[serde(default)]
    replace_all: Option<Regex>,

    #[serde(default)]
    prepend: bool,

    #[serde(default)]
    append: bool,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[cfg(feature = "tera")]
    #[error(transparent)]
    Tera(#[from] tera::Error),
    #[cfg(feature = "minijinja")]
    #[error(transparent)]
    MiniJinja(#[from] minijinja::Error),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    YAML(#[from] serde_yaml::Error),
    #[error(transparent)]
    Glob(#[from] glob::PatternError),
    #[error(transparent)]
    Any(Box<dyn std::error::Error + Send + Sync>),
}
type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum GenResult {
    Skipped,
    Generated { message: Option<String> },
}

/// Split `input` into chunks according to `---` separator and return a vec of pairs of frontmatter and body.
///
/// # Errors
///
/// This function will return an error if operation fails
fn parse_template(input: &str) -> Result<Vec<(FrontMatter, String)>> {
    // normalize line endings
    let input = input.replace("\r\n", "\n");
    debug!("input: {input:?}");
    let mut parts: Vec<&str> = input.split("---\n").collect();
    debug!("parts: {parts:?}");

    if parts.len()!=2 {
        parts = parts.into_iter().filter(|part| !part.replace("\n","").replace("\t","").eq("")).collect();
        debug!("new parts: {parts:?}");
    }

    let parts_split: Result<Vec<(FrontMatter, String)>> = parts.chunks(2)
        .map(|chunk| {
            if chunk.len() != 2 {
                return Err(Error::Message("cannot split document into pair(s) of frontmatter and body".to_string()));
            }
            let fm = chunk[0];
            let body = chunk[1];
            debug!("frontmatter: {fm:?}");
            debug!("body: {body:?}");
            let front_matter: FrontMatter = serde_yaml::from_str(fm)?;
            Ok((front_matter, body.to_string()))
        })
        .collect();

    parts_split
}

pub struct RRgen {
    working_dir: Option<PathBuf>,
    fs: Box<dyn FsDriver>,
    printer: Box<dyn Printer>,
    #[cfg(feature = "tera")]
    tera: Tera,
    #[cfg(feature = "minijinja")]
    minijinja: Environment<'static>,
}

impl Default for RRgen {
    fn default() -> Self {
        #[cfg(feature = "tera")]
        let mut tera_instance = Tera::default();
        #[cfg(feature = "tera")]
        tera_filters::register_all(&mut tera_instance);

        #[cfg(feature = "minijinja")]
        let mut minijinja = Environment::new();
        #[cfg(feature = "minijinja")]
        minijinja_filters::register_all(&mut minijinja);

        Self {
            working_dir: None,
            fs: Box::new(RealFsDriver {}),
            printer: Box::new(ConsolePrinter {}),
            #[cfg(feature = "tera")]
            tera: tera_instance,
            #[cfg(feature = "minijinja")]
            minijinja,
        }
    }
}

impl RRgen {
    /// Creates a new [`RRgen`] instance with the specified working directory.
    ///
    /// # Example
    /// ```rust
    /// use rrgen::RRgen;
    ///
    /// let rgen = RRgen::with_working_dir("path");
    ///
    /// ```
    #[must_use]
    pub fn with_working_dir<P: AsRef<Path>>(path: P) -> Self {
        Self {
            working_dir: Some(path.as_ref().to_path_buf()),
            ..Default::default()
        }
    }

    /// Creates a new `RRgen` instance with the specified templates.
    ///
    /// # Example
    /// ```rust
    /// use rrgen::RRgen;
    /// use std::collections::HashMap;
    ///
    /// let templates = vec![
    ///     ("template1", "content of template 1"),
    ///     ("template2", "content of template 2"),
    /// ];
    /// let rgen = RRgen::with_templates(templates).unwrap();
    ///
    /// let mut map = HashMap::new();
    /// map.insert("template3", "content of template 3");
    /// let rgen = RRgen::with_templates(map).unwrap();
    /// ```
    ///
    /// # Errors
    ///
    /// This function will return an error if operation fails
    pub fn with_templates<'a, I>(templates: I) -> std::result::Result<Self, Error>
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let mut rgen = RRgen::default();
        for (name, content) in templates {
            rgen.add_template(name, content)?;
        }
        Ok(rgen)
    }

    /// Generate from a template contained in `input`
    ///
    /// # Errors
    ///
    /// This function will return an error if operation fails
    pub fn generate(&self, input: &str, vars: &serde_json::Value) -> Result<GenResult> {
        debug!("generating from template: {input:?}");
        debug!("template vars: {:?}", serde_json::to_string(&vars)?);
        #[cfg(feature = "tera")]{
            let mut tera = self.tera.clone();
            let rendered = tera.render_str(input, &Context::from_serialize(vars.clone())?)?;
            debug!("rendered: {rendered:?}");
            self.handle_rendered(rendered.as_str())
        }
        #[cfg(feature = "minijinja")]{
            let rendered = self.minijinja.render_str(input, vars.clone())?;
            self.handle_rendered(rendered.as_str())
        }
    }

    /// Generate from a template added in the template engine given by `name`
    ///
    /// # Errors
    ///
    /// This function will return an error if operation fails
    pub fn generate_by_template_with_name(&self, name: &str, vars: &serde_json::Value) -> Result<GenResult> {
        debug!("generating from template with name: {name:?}, vars: {:?}",serde_json::to_string(&vars)?);

        #[cfg(feature = "tera")]{
            let rendered = self.tera.render(name, &Context::from_serialize(vars.clone())?)?;
            debug!("rendered: {rendered:?}");
            self.handle_rendered(&rendered)
        }

        #[cfg(feature = "minijinja")]{
            let template = self.minijinja.get_template(name);
            let rendered = template?.render(vars)?;
            self.handle_rendered(rendered.as_str())
        }
    }

    /// Add template with the given name in template engine
    ///
    /// # Errors
    ///
    /// This function will return an error if operation fails
    fn add_template(&mut self, name: &str, template: &str) -> Result<()> {
        #[cfg(feature = "tera")]{
            self.tera.add_raw_template(name, template)?
        }
        #[cfg(feature = "minijinja")]{
            self.minijinja.add_template_owned(name.to_string(), template.to_string())?
        }
        Ok(())
    }

    /// Handle rendered string by splitting to pairs of frontmatter and body and then handle frontmatter
    ///
    /// # Errors
    ///
    /// This function will return an error if operation fails
    fn handle_rendered(&self, rendered: &str) -> Result<GenResult> {
        debug!("rendered: {rendered:?}");
        let parts = parse_template(rendered)?;
        let messages: Vec<String> = parts.iter()
            .map(|(front_matter, body)| self.handle_frontmatter_and_body(front_matter.clone(), &body))
            .collect::<Result<Vec<GenResult>>>()?
            .into_iter()
            .filter_map(|gen_result| {
                if let GenResult::Generated { message: Some(msg) } = gen_result {
                    Some(msg)
                } else {
                    None
                }
            })
            .collect();

        let merged_message = messages.join("\n");
        Ok(GenResult::Generated { message: Some(merged_message) })
    }

    /// Handle frontmatter and body
    ///
    /// # Errors
    ///
    /// This function will return an error if operation fails
    fn handle_frontmatter_and_body(&self, frontmatter: FrontMatter, body: &str) -> Result<GenResult> {
        if frontmatter.to.is_some() {
            let path_to = if let Some(working_dir) = &self.working_dir {
                working_dir.join(frontmatter.to.unwrap())
            } else {
                PathBuf::from(&frontmatter.to.unwrap())
            };

            if frontmatter.skip_exists && self.fs.exists(&path_to) {
                self.printer.skip_exists(&path_to);
                return Ok(GenResult::Skipped);
            }
            if let Some(skip_glob) = frontmatter.skip_glob {
                if glob::glob(&skip_glob)?.count() > 0 {
                    self.printer.skip_exists(&path_to);
                    return Ok(GenResult::Skipped);
                }
            }

            if self.fs.exists(&path_to) {
                self.printer.overwrite_file(&path_to);
            } else {
                self.printer.add_file(&path_to);
            }
            // write main file
            self.fs.write_file(&path_to, &body)?;
        }

        // handle injects
        self.handle_injects(frontmatter.injections, frontmatter.message.clone())?;
        Ok(GenResult::Generated {
            message: frontmatter.message.clone(),
        })
    }

    fn handle_injects(&self, injections: Option<Vec<Injection>>, message: Option<String>) -> Result<GenResult> {
        if let Some(injections) = injections {
            for injection in &injections {
                let injection_to = self.working_dir.as_ref().map_or_else(
                    || PathBuf::from(&injection.into),
                    |working_dir| working_dir.join(&injection.into),
                );
                if !self.fs.exists(&injection_to) {
                    return Err(Error::Message(format!(
                        "cannot inject into {}: file does not exist",
                        injection.into,
                    )));
                }

                let file_content = self.fs.read_file(&injection_to)?;
                let content = &injection.content;

                if let Some(skip_if) = &injection.skip_if {
                    if skip_if.is_match(&file_content) {
                        continue;
                    }
                }

                let new_content = if injection.prepend {
                    format!("{content}\n{file_content}")
                } else if injection.append {
                    format!("{file_content}\n{content}")
                } else if let Some(before) = &injection.before {
                    insert_content_at_positions(&file_content, content, injection.inline, before, First, InsertionPoint::Before)
                } else if let Some(before_last) = &injection.before_last {
                    insert_content_at_positions(&file_content, content, injection.inline, before_last, Last, InsertionPoint::Before)
                } else if let Some(before_last) = &injection.before_all {
                    insert_content_at_positions(&file_content, content, injection.inline, before_last, All, InsertionPoint::Before)
                } else if let Some(after) = &injection.after {
                    insert_content_at_positions(&file_content, content, injection.inline, after, First, InsertionPoint::After)
                } else if let Some(after_last) = &injection.after_last {
                    insert_content_at_positions(&file_content, content, injection.inline, after_last, Last, InsertionPoint::After)
                } else if let Some(after_all) = &injection.after_all {
                    insert_content_at_positions(&file_content, content, injection.inline, after_all, All, InsertionPoint::After)
                } else if let Some(remove_lines) = &injection.remove_lines {
                    let lines = file_content
                        .lines()
                        .filter(|line| !remove_lines.is_match(line))
                        .collect::<Vec<_>>();
                    lines.join("\n")
                } else if let Some(replace) = &injection.replace {
                    replace
                        .replace(&file_content, content.as_str())
                        .to_string()
                } else if let Some(replace) = &injection.replace_all {
                    replace
                        .replace_all(&file_content, content.as_str())
                        .to_string()
                } else {
                    println!("warning: no injection made");
                    file_content.clone()
                };

                self.fs.write_file(&injection_to, &new_content)?;
                self.printer.injected(&injection_to);
            }
        }
        Ok(GenResult::Generated {
            message: message.clone(),
        })
    }
}
#[derive(Debug, Clone)]
enum MatchPositions {
    All,
    First,
    Last,
}

fn find_positions(lines: Vec<&str>, regex: &Regex, input: &MatchPositions) -> Vec<usize> {
    let matching_positions: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| if regex.is_match(line) { Some(i) } else { None })
        .collect();

    match input {
        All => matching_positions,
        First => matching_positions.into_iter().take(1).collect(),
        Last => matching_positions.into_iter().rev().take(1).collect(),
    }
}
#[derive(Debug)]
enum InsertionPoint {
    Before,
    After,
}

fn insert_content_at_positions(
    file_content: &str,
    content: &str,
    inline: bool,
    regex: &Regex,
    match_positions: MatchPositions,
    position: InsertionPoint,
) -> String {
    let lines: Vec<String> = file_content.split('\n').map(|line| line.to_string()).collect();
    let lines = file_content.lines().collect::<Vec<_>>();
    let positions = find_positions(lines.clone(), regex, &match_positions);

    let replace_with = |caps: &regex::Captures| {
        match position {
            InsertionPoint::Before => format!("{}{}", content, &caps[0]),
            InsertionPoint::After => format!("{}{}", &caps[0], content),
        }
    };

    let new_lines = lines.iter()
        .enumerate()
        .flat_map(|(index,line)| {
        if regex.is_match(line) && positions.contains(&index) {
            if inline {
                let new_line = match match_positions {
                    MatchPositions::All => regex.replace_all(line, replace_with).to_string(),
                    MatchPositions::First => regex.replace(line, replace_with).to_string(),
                    MatchPositions::Last => {
                        let count = regex.find_iter(line).count();
                        regex.replacen(line, count, replace_with).to_string()
                    }
                };
                vec![new_line]
            } else {
                if matches!(position, InsertionPoint::Before) {
                    vec![content.to_string(), line.to_string()]
                } else {
                    vec![line.to_string(), content.to_string()]
                }
            }
        } else {
            vec![line.to_string()]
        }
    }).collect::<Vec<String>>();
    new_lines.join("\n")
}