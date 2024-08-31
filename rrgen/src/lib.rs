use std::fs;
use std::path::Path;

use regex::Regex;
use serde::Deserialize;
use tera::{Context, Tera};
use log::{debug, error};

mod tera_filters;
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

#[derive(Deserialize, Debug, Default)]
struct FrontMatter {
    to:Option<String>,

    #[serde(default)]
    skip_exists: bool,

    #[serde(default)]
    skip_glob: Option<String>,

    #[serde(default)]
    message: Option<String>,

    #[serde(default)]
    injections: Option<Vec<Injection>>,
}

#[derive(Deserialize, Debug, Default)]
struct Injection {
    into: String,
    content: String,

    #[serde(with = "serde_regex")]
    #[serde(default)]
    skip_if: Option<Regex>,

    #[serde(with = "serde_regex")]
    #[serde(default)]
    before: Option<Regex>,

    #[serde(with = "serde_regex")]
    #[serde(default)]
    before_last: Option<Regex>,

    #[serde(with = "serde_regex")]
    #[serde(default)]
    after: Option<Regex>,

    #[serde(with = "serde_regex")]
    #[serde(default)]
    after_last: Option<Regex>,

    #[serde(with = "serde_regex")]
    #[serde(default)]
    remove_lines: Option<Regex>,

    #[serde(default)]
    prepend: bool,

    #[serde(default)]
    append: bool,

    #[serde(default)]
    create_if_missing: bool,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Tera(#[from] tera::Error),
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

pub enum GenResult {
    Skipped,
    Generated { message: Option<String> },
}

fn parse_template(input: &str, delimeter: &str) -> Result<(FrontMatter, String)> {
    let (fm, body) = input.trim().split_once(delimeter).ok_or_else(|| {
        debug!("input:{input}");
        debug!("delimeter:{delimeter}");
        Error::Message("cannot split document to frontmatter and body".to_string())
    })?;
    let frontmatter: FrontMatter = serde_yaml::from_str(fm)?;
    Ok((frontmatter, body.to_string()))
}
pub struct RRgen {
    fs: Box<dyn FsDriver>,
    printer: Box<dyn Printer>,
}

impl Default for RRgen {
    fn default() -> Self {
        Self {
            fs: Box::new(RealFsDriver {}),
            printer: Box::new(ConsolePrinter {}),
        }
    }
}

impl RRgen {
    /// Generate from a template contained in `input`
    ///
    /// # Errors
    ///
    /// This function will return an error if operation fails
    pub fn generate(&self, input: &str, vars: &serde_json::Value) -> Result<()> {
        let delimeter = vars.get("frontmatterSeparator").and_then(|v| v.as_str()).unwrap_or("===\n").to_string();
        let document_separator = vars.get("documentSeparator").and_then(|v| v.as_str()).unwrap_or("---\n").to_string();
        let mut tera = Tera::default();
        tera_filters::register_all(&mut tera);
        // debug!("input: {input:?}");
        let rendered = tera.render_str(input, &Context::from_serialize(vars.clone())?)?;
        // debug!("rendered: {rendered:?}");
        let documents = rendered.split(&document_separator).filter(move |x| x.trim().len() > 1).for_each(move |document| {
            // debug!("document: {document:?}");
            self.gen_result(document, &delimeter).unwrap();
        });
        Ok(())
    }

    pub fn gen_result(&self, rendered: &str, delimeter: &str) -> Result<GenResult> {
        // debug!("rendered: {:?}", rendered);
        let (frontmatter, body) = parse_template(&rendered, &delimeter)?;
        // debug!("frontmatter: {:?}", frontmatter);
        // debug!("body: {:?}", body);


        if let Some(to) = frontmatter.to {
            let path_to = Path::new(&to);
            if frontmatter.skip_exists && self.fs.exists(path_to) {
                self.printer.skip_exists(path_to);
                return Ok(GenResult::Skipped);
            }
            //TODO: Skip only if the content is different
            if let Some(skip_glob) = frontmatter.skip_glob {
                if glob::glob(&skip_glob)?.count() > 0 {
                    self.printer.skip_exists(path_to);
                    return Ok(GenResult::Skipped);
                }
            }

            if self.fs.exists(path_to) {
                self.printer.overwrite_file(path_to);
            } else {
                self.printer.add_file(path_to);
            }
            // write main file
            self.fs.write_file(path_to, &body)?;
        }

        // handle injects
        // since injection is a dependency for another file wait for it to be created
        //TODO check if  injections already exist and do it if not
        if let Some(injections) = frontmatter.injections {
            for injection in &injections {
                let injection_to = Path::new(&injection.into);
                if !self.fs.exists(injection_to) {
                    if (injection.create_if_missing) {
                        fs::File::create(injection.into.clone())?;
                    } else {
                        return Err(Error::Message(format!(
                            "cannot inject into {}: file does not exist",
                            injection.into,
                        )));
                    }
                }

                let file_content = self.fs.read_file(injection_to)?;
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
                    let mut lines = file_content.lines().collect::<Vec<_>>();
                    let pos = lines.iter().position(|ln| before.is_match(ln));
                    if let Some(pos) = pos {
                        lines.insert(pos, content);
                    }
                    lines.join("\n")
                } else if let Some(before_last) = &injection.before_last {
                    let mut lines = file_content.lines().collect::<Vec<_>>();
                    let pos = lines.iter().rposition(|ln| before_last.is_match(ln));
                    if let Some(pos) = pos {
                        lines.insert(pos, content);
                    }
                    lines.join("\n")
                } else if let Some(after) = &injection.after {
                    let mut lines = file_content.lines().collect::<Vec<_>>();
                    let pos = lines.iter().position(|ln| after.is_match(ln));
                    if let Some(pos) = pos {
                        lines.insert(pos + 1, content);
                    }
                    lines.join("\n")
                } else if let Some(after_last) = &injection.after_last {
                    let mut lines = file_content.lines().collect::<Vec<_>>();
                    let pos = lines.iter().rposition(|ln| after_last.is_match(ln));
                    if let Some(pos) = pos {
                        lines.insert(pos + 1, content);
                    }
                    lines.join("\n")
                } else if let Some(remove_lines) = &injection.remove_lines {
                    let lines = file_content
                        .lines()
                        .filter(|line| !remove_lines.is_match(line))
                        .collect::<Vec<_>>();
                    lines.join("\n")
                } else {
                    println!("warning: no injection made");
                    file_content.clone()
                };

                self.fs.write_file(injection_to, &new_content)?;
                self.printer.injected(injection_to);
            }
        }
        Ok(GenResult::Generated {
            message: frontmatter.message.clone(),
        })
    }
}
