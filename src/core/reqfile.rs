//! The Reqfile.yaml format, parsed into precise types.

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

use super::error::ConfigError;
use super::paths;
use super::yaml::{self, Node, Value};

pub const FILE_NAME: &str = "Reqfile.yaml";
const VERSION: i64 = 1;
const DEFAULT_TIMEOUT_SECS: u64 = 60;

/// A file whose name looks like a Reqfile but is not exactly `Reqfile.yaml`.
pub fn is_near_miss(path: &str) -> bool {
    let name = paths::file_name(path);
    name != FILE_NAME && matches!(name.to_lowercase().as_str(), "reqfile.yaml" | "reqfile.yml")
}

/// YAML with a top-level Reqfile key, so a near-miss name holding it is a
/// misnamed Reqfile rather than another tool's file, such as a CI workflow
/// named `reqfile.yml`.
pub fn looks_like_reqfile(text: &str) -> bool {
    match yaml::parse("", text).map(|node| node.value) {
        Ok(Value::Map(entries)) => entries
            .iter()
            .any(|(key, _)| matches!(key.name.as_str(), "reqfile" | "product" | "code")),
        _ => false,
    }
}

#[derive(Debug)]
pub struct Reqfile {
    pub path: String,
    pub dir: String,
    pub requirements: Vec<Requirement>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Product,
    Code,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Product => "product",
            Kind::Code => "code",
        }
    }
}

#[derive(Debug)]
pub struct Requirement {
    pub id: String,
    pub kind: Kind,
    pub must: String,
    pub why: String,
    pub line: usize,
    pub checks: Vec<Check>,
}

#[derive(Debug)]
pub enum Check {
    Command(CommandCheck),
    Decision(DecisionCheck),
}

#[derive(Debug)]
pub struct CommandCheck {
    pub run: String,
    /// Declared quick enough for `reqfile check --fast`, such as in an edit hook.
    pub fast: bool,
    pub fix_hint: String,
    pub files: Option<FileGlob>,
    pub pass_files: bool,
    pub format: OutputFormat,
    pub violation_codes: Vec<i32>,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputFormat {
    Exit,
    Sarif,
}

#[derive(Debug)]
pub struct DecisionCheck {
    pub blocking: bool,
    pub line: usize,
}

/// Globs relative to the folder of their Reqfile; a path matches if any glob does.
#[derive(Debug, Clone)]
pub struct FileGlob {
    set: GlobSet,
}

impl FileGlob {
    pub fn new(pattern: &str) -> Result<Self, String> {
        Self::any(&[pattern])
    }

    pub fn any(patterns: &[&str]) -> Result<Self, String> {
        let mut set = GlobSetBuilder::new();
        for pattern in patterns {
            set.add(
                GlobBuilder::new(pattern)
                    .literal_separator(true)
                    .build()
                    .map_err(|e| e.to_string())?,
            );
        }
        Ok(Self {
            set: set.build().map_err(|e| e.to_string())?,
        })
    }

    pub fn is_match(&self, relative_path: &str) -> bool {
        self.set.is_match(relative_path)
    }
}

pub fn parse(path: &str, text: &str) -> Result<Reqfile, ConfigError> {
    let root = yaml::parse(path, text)?;
    const NO_VERSION: &str = "a Reqfile needs a `reqfile: 1` version key";
    if matches!(root.value, Value::Null) {
        return Err(ConfigError::at(path, 1, NO_VERSION));
    }
    let mut fields = root.fields(path, "the Reqfile", &["reqfile", "product", "code"])?;
    let file_line = fields.line;
    let version = fields
        .optional("reqfile")
        .ok_or_else(|| ConfigError::at(path, file_line, NO_VERSION))?;
    let line = version.line;
    let version = version.integer(path, "reqfile")?;
    if version != VERSION {
        return Err(ConfigError::at(
            path,
            line,
            format!(
                "unsupported Reqfile version {version}; this reqfile supports version {VERSION}"
            ),
        ));
    }
    let mut requirements = Vec::new();
    for kind in [Kind::Product, Kind::Code] {
        if let Some(list) = fields.optional(kind.as_str()) {
            for node in list.list(path, kind.as_str())? {
                requirements.push(requirement(path, kind, node)?);
            }
        }
    }
    Ok(Reqfile {
        path: path.to_string(),
        dir: paths::parent(path).to_string(),
        requirements,
    })
}

fn requirement(path: &str, kind: Kind, node: Node) -> Result<Requirement, ConfigError> {
    let line = node.line;
    let mut fields = node.fields(
        path,
        "a requirement",
        &["id", "ref", "must", "why", "who", "checks"],
    )?;
    let id = fields.required("id")?;
    let id_line = id.line;
    let id = id.text(path, "id")?;
    if !is_screaming_snake_case(&id) {
        return Err(ConfigError::at(
            path,
            id_line,
            format!("requirement id `{id}` must be SCREAMING_SNAKE_CASE"),
        ));
    }
    fields.what = format!("requirement {id}");
    let must = fields.required("must")?.text(path, "must")?;
    let why = fields.required("why")?.text(path, "why")?;
    if let Some(who) = fields.optional("who") {
        who.text(path, "who")?;
    }
    if let Some(reference) = fields.optional("ref") {
        reference.text(path, "ref")?;
    }
    let checks_node = fields.required("checks")?;
    let checks_line = checks_node.line;
    let mut checks = Vec::new();
    for check_node in checks_node.list(path, "checks")? {
        let check = check(path, check_node)?;
        if let (Check::Decision(new), true) = (
            &check,
            checks.iter().any(|c| matches!(c, Check::Decision(_))),
        ) {
            return Err(ConfigError::at(
                path,
                new.line,
                format!("requirement {id} declares more than one decision check"),
            ));
        }
        checks.push(check);
    }
    if checks.is_empty() {
        return Err(ConfigError::at(
            path,
            checks_line,
            format!(
                "requirement {id} declares no checks; add at least one `command` or `decision` check"
            ),
        ));
    }
    Ok(Requirement {
        id,
        kind,
        must,
        why,
        line,
        checks,
    })
}

fn is_screaming_snake_case(id: &str) -> bool {
    id.starts_with(|c: char| c.is_ascii_uppercase())
        && !id.ends_with('_')
        && !id.contains("__")
        && id
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

const NOT_A_CHECK: &str = "a check is either `decision` or a `command` mapping";

fn check(path: &str, node: Node) -> Result<Check, ConfigError> {
    let line = node.line;
    match node.value {
        Value::Str(name) if name == "decision" => Ok(Check::Decision(DecisionCheck {
            blocking: false,
            line,
        })),
        Value::Map(mut entries) if entries.len() == 1 => {
            let (key, body) = entries.remove(0);
            match key.name.as_str() {
                "command" => command(path, body).map(Check::Command),
                "decision" => decision(path, line, body).map(Check::Decision),
                other => Err(ConfigError::at(
                    path,
                    key.line,
                    format!("unknown check `{other}`; {NOT_A_CHECK}"),
                )),
            }
        }
        Value::Str(other) => Err(ConfigError::at(
            path,
            line,
            format!("unknown check `{other}`; {NOT_A_CHECK}"),
        )),
        _ => Err(ConfigError::at(path, line, NOT_A_CHECK)),
    }
}

fn decision(path: &str, line: usize, body: Node) -> Result<DecisionCheck, ConfigError> {
    if matches!(body.value, Value::Null) {
        return Ok(DecisionCheck {
            blocking: false,
            line,
        });
    }
    let mut fields = body.fields(path, "a decision check", &["mode"])?;
    let blocking = match fields.optional("mode") {
        None => false,
        Some(mode) => {
            let mode_line = mode.line;
            match mode.text(path, "mode")?.as_str() {
                "advisory" => false,
                "blocking" => true,
                other => {
                    return Err(ConfigError::at(
                        path,
                        mode_line,
                        format!(
                            "unknown decision mode `{other}`; expected `advisory` or `blocking`"
                        ),
                    ));
                }
            }
        }
    };
    Ok(DecisionCheck { blocking, line })
}

fn command(path: &str, body: Node) -> Result<CommandCheck, ConfigError> {
    let mut fields = body.fields(
        path,
        "a command check",
        &[
            "run",
            "fix_hint",
            "files",
            "pass_files",
            "format",
            "violation_codes",
            "timeout",
            "fast",
        ],
    )?;
    let run = fields.required("run")?.text(path, "run")?;
    let fix_hint = fields.required("fix_hint")?.text(path, "fix_hint")?;
    let files =
        match fields.optional("files") {
            None => None,
            Some(node) => {
                let line = node.line;
                let pattern = node.text(path, "files")?;
                Some(FileGlob::new(&pattern).map_err(|e| {
                    ConfigError::at(path, line, format!("invalid `files` glob: {e}"))
                })?)
            }
        };
    let pass_files = match fields.optional("pass_files") {
        None => false,
        Some(node) => {
            let line = node.line;
            let pass = node.boolean(path, "pass_files")?;
            if pass && files.is_none() {
                return Err(ConfigError::at(
                    path,
                    line,
                    "`pass_files` needs a `files` glob",
                ));
            }
            pass
        }
    };
    let fast = match fields.optional("fast") {
        None => false,
        Some(node) => node.boolean(path, "fast")?,
    };
    let format = match fields.optional("format") {
        None => OutputFormat::Exit,
        Some(node) => {
            let line = node.line;
            match node.text(path, "format")?.as_str() {
                "exit" => OutputFormat::Exit,
                "sarif" => OutputFormat::Sarif,
                other => {
                    return Err(ConfigError::at(
                        path,
                        line,
                        format!("unknown format `{other}`; expected `exit` or `sarif`"),
                    ));
                }
            }
        }
    };
    let violation_codes = match fields.optional("violation_codes") {
        None => vec![1],
        Some(node) => {
            let line = node.line;
            let codes = node
                .list(path, "violation_codes")?
                .into_iter()
                .map(|code| code.integer(path, "violation_codes"))
                .collect::<Result<Vec<_>, _>>()?;
            if codes.is_empty() || codes.iter().any(|c| !(1..=255).contains(c)) {
                return Err(ConfigError::at(
                    path,
                    line,
                    "`violation_codes` must list exit codes between 1 and 255",
                ));
            }
            codes.into_iter().map(|c| c as i32).collect()
        }
    };
    let timeout_secs = match fields.optional("timeout") {
        None => DEFAULT_TIMEOUT_SECS,
        Some(node) => {
            let line = node.line;
            match node.integer(path, "timeout")? {
                secs if secs > 0 => secs as u64,
                _ => {
                    return Err(ConfigError::at(
                        path,
                        line,
                        "`timeout` must be a positive number of seconds",
                    ));
                }
            }
        }
    };
    Ok(CommandCheck {
        run,
        fast,
        fix_hint,
        files,
        pass_files,
        format,
        violation_codes,
        timeout_secs,
    })
}
