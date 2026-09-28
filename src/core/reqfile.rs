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
        Ok(Value::Map(entries)) => entries.iter().any(|(key, _)| {
            key.name == "reqfile" || Kind::ALL.iter().any(|k| k.as_str() == key.name)
        }),
        _ => false,
    }
}

#[derive(Debug)]
pub struct Reqfile {
    pub path: String,
    pub dir: String,
    /// Ids are unique within a Reqfile.
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Product,
    Code,
    Process,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Product, Kind::Code, Kind::Process];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Product => "product",
            Kind::Code => "code",
            Kind::Process => "process",
        }
    }

    /// A product requirement describes its own repository's product, so
    /// only code and process requirements can be taken by reference.
    pub fn importable(self) -> bool {
        self != Kind::Product
    }
}

/// One entry of a Reqfile: a definition, or a use block taking its
/// requirement by reference from another folder or repository.
#[derive(Debug, Clone)]
pub struct Block {
    pub id: String,
    pub kind: Kind,
    pub line: usize,
    /// Required in a definition; in a use block, replaces the definition's.
    pub why: Option<String>,
    pub who: Option<String>,
    /// Required in a definition; in a use block, replaces the definition's
    /// checks entirely.
    pub checks: Option<Checks>,
    pub origin: Origin,
}

#[derive(Debug, Clone)]
pub enum Origin {
    Definition {
        must: String,
        reference: Option<String>,
    },
    Use(Location),
}

/// The checks of a block, with their YAML as written, which identifies them.
#[derive(Debug, Clone)]
pub struct Checks {
    pub list: Vec<Check>,
    pub source: String,
}

/// Where a use block takes its requirement from.
#[derive(Debug, Clone, PartialEq)]
pub enum Location {
    /// A folder, relative to the folder of the Reqfile, as written.
    Local(String),
    /// A repository `owner/repo`, pinned to a ref.
    Git { repo: String, reference: GitRef },
}

impl Location {
    pub fn describe(&self) -> String {
        match self {
            Location::Local(path) => path.clone(),
            Location::Git { repo, reference } => format!("{repo}@{}", reference.as_str()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GitRef {
    /// A full 40-character commit id.
    Commit(String),
    /// A tag, resolved through `refs/tags/<name>`.
    Tag(String),
}

impl GitRef {
    pub fn as_str(&self) -> &str {
        match self {
            GitRef::Commit(id) | GitRef::Tag(id) => id,
        }
    }
}

/// Parses `use`: `./folder`, `../folder` or `owner/repo@ref`.
pub fn parse_location(text: &str) -> Result<Location, String> {
    if text == "." || text == ".." || text.starts_with("./") || text.starts_with("../") {
        return Ok(Location::Local(text.to_string()));
    }
    let invalid = || {
        format!(
            "`use: {text}` must be a folder relative to this Reqfile (./std) or a repository pinned to a tag or commit (owner/repo@v1.0.0)"
        )
    };
    let (repo, reference) = text.split_once('@').ok_or_else(invalid)?;
    let name = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    match repo.split_once('/') {
        Some((owner, name_part)) if name(owner) && name(name_part) => {}
        _ => return Err(invalid()),
    }
    Ok(Location::Git {
        repo: repo.to_string(),
        reference: parse_ref(reference)?,
    })
}

/// A full commit id or a tag name; `HEAD` and short commit ids are rejected
/// here, and a name that is not a tag (a branch) when it is resolved.
fn parse_ref(text: &str) -> Result<GitRef, String> {
    let hex = !text.is_empty() && text.chars().all(|c| c.is_ascii_hexdigit());
    if text.len() == 40 && hex {
        Ok(GitRef::Commit(text.to_ascii_lowercase()))
    } else if text.is_empty() || text == "HEAD" || text.starts_with("refs/") {
        Err(format!(
            "`{text}` is not a tag or a full commit id; pin requirements to a tag (v1.0.0) or a 40-character commit id"
        ))
    } else if hex && text.len() >= 7 && text.chars().any(|c| c.is_ascii_digit()) {
        Err(format!(
            "`{text}` looks like a short commit id; use the full 40-character id so the pin cannot become ambiguous"
        ))
    } else {
        Ok(GitRef::Tag(text.to_string()))
    }
}

#[derive(Debug, Clone)]
pub enum Check {
    Command(CommandCheck),
    Decision(DecisionCheck),
}

#[derive(Debug, Clone)]
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
    /// How results carrying a probability of violation are judged.
    pub thresholds: Thresholds,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputFormat {
    Exit,
    Sarif,
    Junit,
}

/// A probability of violation above `violation_above` is a violation, below
/// `pass_below` a pass, and anything between an uncertain finding.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    pub violation_above: f64,
    pub pass_below: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            violation_above: 0.8,
            pass_below: 0.2,
        }
    }
}

#[derive(Debug, Clone)]
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
    let mut fields = root.fields(
        path,
        "the Reqfile",
        &["reqfile", "product", "code", "process"],
    )?;
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
    let mut blocks: Vec<Block> = Vec::new();
    for kind in Kind::ALL {
        if let Some(list) = fields.optional(kind.as_str()) {
            for node in list.list(path, kind.as_str())? {
                let block = block(path, kind, node)?;
                if let Some(first) = blocks.iter().find(|b| b.id == block.id) {
                    return Err(ConfigError::at(
                        path,
                        block.line,
                        format!(
                            "requirement id {} appears twice in this Reqfile (first at line {})",
                            block.id, first.line
                        ),
                    ));
                }
                blocks.push(block);
            }
        }
    }
    Ok(Reqfile {
        path: path.to_string(),
        dir: paths::parent(path).to_string(),
        blocks,
    })
}

/// A block with `use` is a use block; any other block is a definition.
fn block(path: &str, kind: Kind, node: Node) -> Result<Block, ConfigError> {
    let line = node.line;
    let mut fields = node.fields(
        path,
        "a requirement",
        &["id", "use", "ref", "must", "why", "who", "checks"],
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
    let location = match fields.optional("use") {
        None => None,
        Some(node) => {
            let use_line = node.line;
            if !kind.importable() {
                return Err(ConfigError::at(
                    path,
                    use_line,
                    format!(
                        "requirement {id} is a product requirement, which describes this repository's own product; only code and process requirements can be taken with `use`"
                    ),
                ));
            }
            let text = node.text(path, "use")?;
            let location = parse_location(&text).map_err(|e| ConfigError::at(path, use_line, e))?;
            if let Location::Git {
                repo,
                reference: GitRef::Tag(tag),
            } = &location
            {
                return Err(ConfigError::at(
                    path,
                    use_line,
                    format!(
                        "`use: {text}` is pinned to a tag, which can move; a Reqfile pins a repository to a full commit id. `reqfile add {repo}@{tag}` writes the line pinned to the commit {tag} points to"
                    ),
                ));
            }
            Some(location)
        }
    };
    let origin = match location {
        Some(location) => {
            for key in ["must", "ref"] {
                if let Some(node) = fields.optional(key) {
                    return Err(ConfigError::at(
                        path,
                        node.line,
                        format!(
                            "requirement {id} is a use block, which takes `{key}` from its definition; remove `{key}`, or remove `use` to define the requirement here"
                        ),
                    ));
                }
            }
            Origin::Use(location)
        }
        None => {
            let must = fields.required("must")?.text(path, "must")?;
            let reference = fields
                .optional("ref")
                .map(|r| r.text(path, "ref"))
                .transpose()?;
            Origin::Definition { must, reference }
        }
    };
    let definition = matches!(origin, Origin::Definition { .. });
    let why = match fields.optional("why") {
        Some(node) => Some(node.text(path, "why")?),
        None if definition => return Err(fields.missing("why")),
        None => None,
    };
    let who = fields
        .optional("who")
        .map(|n| n.text(path, "who"))
        .transpose()?;
    let checks = match fields.optional("checks") {
        Some(node) => Some(checks(path, &id, node)?),
        None if definition => return Err(fields.missing("checks")),
        None => None,
    };
    Ok(Block {
        id,
        kind,
        line,
        why,
        who,
        checks,
        origin,
    })
}

fn checks(path: &str, id: &str, checks_node: Node) -> Result<Checks, ConfigError> {
    let checks_line = checks_node.line;
    let source = checks_node.clone().into_json().to_string();
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
    Ok(Checks {
        list: checks,
        source,
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
            "thresholds",
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
                "junit" => OutputFormat::Junit,
                other => {
                    return Err(ConfigError::at(
                        path,
                        line,
                        format!("unknown format `{other}`; expected `exit`, `sarif` or `junit`"),
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
    let thresholds = match fields.optional("thresholds") {
        None => Thresholds::default(),
        Some(node) => {
            let line = node.line;
            let mut t = node.fields(path, "thresholds", &["violation_above", "pass_below"])?;
            let defaults = Thresholds::default();
            let violation_above = match t.optional("violation_above") {
                None => defaults.violation_above,
                Some(n) => n.number(path, "violation_above")?,
            };
            let pass_below = match t.optional("pass_below") {
                None => defaults.pass_below,
                Some(n) => n.number(path, "pass_below")?,
            };
            if !(0.0..=1.0).contains(&pass_below)
                || !(0.0..=1.0).contains(&violation_above)
                || pass_below > violation_above
            {
                return Err(ConfigError::at(
                    path,
                    line,
                    "thresholds must satisfy 0 <= pass_below <= violation_above <= 1",
                ));
            }
            Thresholds {
                violation_above,
                pass_below,
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
        thresholds,
    })
}
