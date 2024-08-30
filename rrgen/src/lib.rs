use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{anyhow, Result};
use glob::glob;
use regex::Regex;
use serde::Deserialize;
use tera::{Context, Tera};
use log::{info, debug, error};
use std::collections::BTreeMap;

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
        debug!("overwritten: {file_to:?}");
    }

    fn add_file(&self, file_to: &Path) {
        debug!("added: {file_to:?}");
    }

    fn injected(&self, file_to: &Path) {
        debug!("injected: {file_to:?}");
    }

    fn skip_exists(&self, file_to: &Path) {
        debug!("skipped (exists): {file_to:?}");
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

#[derive(Debug)]
pub enum GenResult {
    Skipped,
    Generated { message: Option<String> },
}

fn parse_template(input: &str, delimeter: &str) -> Result<(FrontMatter, String)> {
    let (fm, body) = input.trim().split_once(delimeter).ok_or_else(|| {
        Error::Message("cannot split document to frontmatter and body".to_string())
    })?;
    let frontmatter: FrontMatter = serde_yaml::from_str(fm)?;
    Ok((frontmatter, body.to_string()))
}
pub struct RRgen {
    fs: Box<dyn FsDriver>,
    printer: Box<dyn Printer>,
    pub frontmatter_separator: String,
    pub document_separator: String,
    pub output_directory: String,
    tera_glob_directory: Option<String>,
    tera_glob_pattern: String,
    tera: Tera,
    template_file_extensions: Vec<String>,
    render_only_file_with_extension: bool,
    copy_non_template: bool,
    overwrite: bool,
}

impl Default for RRgen {
    fn default() -> Self {
        let rrgen_tera_glob_directory = env::var("RRGEN_TERA_GLOB_DIRECTORY").ok();
        let mut tera = match &rrgen_tera_glob_directory {
            Some(templates_dir) => Tera::new(templates_dir),
            None => Ok(Tera::default()),
        }.unwrap();
        tera_filters::register_all(&mut tera);
        Self {
            fs: Box::new(RealFsDriver {}),
            printer: Box::new(ConsolePrinter {}),
            frontmatter_separator: env::var("RRGEN_FRONTMATTER_SEPARATOR").unwrap_or("---\n".into()),
            document_separator: env::var("RRGEN_DOCUMENT_SEPARATOR").unwrap_or("===\n".into()),
            output_directory: env::var("RRGEN_OUTPUT_DIRECTORY").unwrap_or(".".into()),
            tera_glob_pattern: env::var("RRGEN_TERA_GLOB_DIRECTORY").unwrap_or("**/_*.tpl".into()),
            tera_glob_directory: rrgen_tera_glob_directory,
            tera: tera,
            template_file_extensions: vec!("rrgen".to_string()),
            render_only_file_with_extension: true,
            copy_non_template: true,
            overwrite: true,
        }
    }
}

impl RRgen {
    pub fn add_templates_to_tera(&mut self, glob: &str){
        self.tera = match Tera::new(glob) {
            Ok(mut tera) => {
                tera_filters::register_all(&mut tera);
                tera
            },
            Err(e) => panic!("Error initializing Tera: {}", e),
        };
    }


    pub fn add_dir_to_tera(&mut self, dir: PathBuf) {
        if !dir.is_dir() {
            panic!("Error: {:?} is not a directory", dir);
        }

        let tera_glob = dir.join(&self.tera_glob_pattern).to_str().unwrap().to_string();

        let template_files = glob(&tera_glob)
            .expect("Failed to read glob pattern")
            .filter_map(|entry| match entry {
                Ok(path) => {
                    if path.is_file() {
                        let filename = path.file_name().unwrap().to_str().unwrap().to_string();
                        Some((path,Some(filename)))
                    } else {
                        None
                    }
                }
                Err(_) => None,
            });

        self.tera.add_template_files(template_files).unwrap();
    }

    /// Traverse all files in `glob` and generate from template contained in each file
    ///
    /// # Errors
    ///
    /// This function will return an error if operation fails
    pub async fn generate_glob(&mut self, dir: &str, vars: &serde_json::Value) -> Result<()> {
        let templates_glob = format!("{}.tpl",dir);
        debug!("templates_glob: {templates_glob}");
        self.add_templates_to_tera(format!("{}.tpl",dir).to_string().as_str());
        let ctx= match Context::from_serialize(vars){
            Ok(ctx) => ctx,
            Err(e) => return Err(anyhow!("cannot get context from vars:{}, error:{}",vars, e)),
        };
        let template_file_extensions = self.template_file_extensions.clone(); // clone before mutable borrow
        let document_separator = self.document_separator.clone();
        let tera = self.tera.clone();

        let files = self.file_tostring_from_dir(dir).await?.clone();
        let template_file_extensions = &self.template_file_extensions.clone();

        let filtered_files= files.iter()
            .filter(|(name, _content)| {
                let file_extension = &name.clone()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .unwrap_or("")
                    .to_string();
                !template_file_extensions.contains(file_extension)
            })
            .for_each(|(name, content)| {
                let binding = name.clone();
                let b2 = dir.clone();
                let relative_path = binding.strip_prefix(b2.strip_suffix("**/*").unwrap())
                    .unwrap_or_else(|_| name);

                let content = content.clone();
                let mut output_file_path = PathBuf::from(format!("{}/{}", self.output_directory, relative_path.display()));
                match self.fs.write_file(&mut output_file_path,&content){
                    Ok(_) => {}
                    Err(_) => error!("cannot write file to output directory:{}, error:{}",relative_path.display(), content),
                }
            });

        let rendered_list = files.iter()
            .filter(|(name, _content)| {
                let file_extension = &name.clone()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .unwrap_or("")
                    .to_string();
                template_file_extensions.contains(file_extension)
            })
            .map(|(name, content)| {
                debug!("rendering file:{}",name.display());
                (name,self.tera.render_str(content, &ctx)
                    .unwrap_or_else(|e| {
                        debug!("error rendering file {:?} due to:{:?},{:?}",name,e,e.kind);
                        String::new()
                    }))
            })
            .filter(|(_name, render)| !render.is_empty())
            .collect::<HashMap<_,_>>();

        let gen_results: Vec<GenResult> = rendered_list
            .iter()
            .flat_map(|(_name, render)| render.split(&document_separator))
            .map(|s| s.to_string())
            .filter(|x| x.trim().len() > 1)
            // .map(move |rendered| {
            //     let (frontmatter, body) = parse_template(&rendered, &self.frontmatter_separator)?;
            //     (frontmatter, body)
            // })
            .map(|doc| self.gen_result(&doc).unwrap_or_else(|e| {
                debug!("error generating document {doc:?} due to:{e}");
                GenResult::Skipped
            }))
            .collect::<Vec<GenResult>>();
        Ok(())
    }

    pub async fn file_tostring_from_dir(&mut self, dir: &str) -> std::result::Result<BTreeMap<PathBuf,String>, anyhow::Error> {
        let files = match glob(dir){
            Ok(glob) => glob,
            Err(e) => return Err(anyhow!("invalid glob pattern: {}", e)),
        };
        debug!("loaded glob.");
        let file_paths: Vec<PathBuf> = files
            .filter_map(|entry| {
                match entry {
                    Ok(path) => Some(path),
                    Err(e) => {
                        error!("Failed to read glob entry: {:?}", e); // Log the error
                        None // Discard the error by returning `None`
                    }
                }
            })
            .collect();

        let files = file_paths.iter()
            .filter( |f|f.exists() && f.is_file())
            .map(|x| {
            let content = fs::read_to_string(x.clone()).unwrap_or_else(|e| {
                debug!("error reading file {x:?} due to:{e}");
                String::new()
            });
            (x.clone(), content.clone())
        })
            .filter(|(_, content)| !content.is_empty())
            .collect::<BTreeMap<_, _>>();
        Ok(files)
    }

    /// Generate from a template contained in `input`
    ///
    /// # Errors
    ///
    /// This function will return an error if operation fails
    pub fn generate(&mut self, input: &str, vars: &serde_json::Value) -> Result<()> {
        // debug!("input: {input:?}");
        let rendered = self.tera.render_str(input, &Context::from_serialize(vars.clone())?)?;
        // debug!("rendered: {rendered:?}");
        rendered.split(&self.document_separator).filter(move |x| x.trim().len() > 1).for_each(|document| {
            // debug!("document: {document:?}");
            self.gen_result(document).unwrap();
        });
        Ok(())

    }

    pub fn gen_result(&self, rendered: &str) -> Result<GenResult> {
        // debug!("rendered: {:?}", rendered);
        let (frontmatter, body) = parse_template(&rendered, &self.frontmatter_separator)?;
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
                    if injection.create_if_missing {
                        fs::File::create(injection.into.clone())?;
                    } else {
                        return Err(anyhow!(
                            "cannot inject into {}: file does not exist",
                            injection.into,
                        ));
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
                    info!("warning: no injection made");
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
