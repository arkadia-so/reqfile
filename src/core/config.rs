//! Settings: `.reqfile/config.yaml` in any folder, applying to that folder
//! and everything under it. Nothing is specific to the repository root: for
//! each key the nearest config wins, and exclusions add up down the tree.

use super::error::ConfigError;
use super::paths;
use super::reqfile::FileGlob;
use super::yaml::{self, Value};

const FILE_NAME: &str = "config.yaml";
/// Jev's latest release on OpenRouter, so decision checks follow new versions without config changes.
const DEFAULT_MODEL: &str = "~typesafe/jev-latest";
const DEFAULT_API_KEY_ENV: &str = "OPENROUTER_API_KEY";
const DEFAULT_ENDPOINT: &str = "https://openrouter.ai/api/v1";
const DEFAULT_CONCURRENCY: usize = 16;

/// The folder a config file configures, if `path` is one.
pub fn config_dir(path: &str) -> Option<&str> {
    let dir = paths::parent(path);
    (paths::file_name(path) == FILE_NAME && paths::file_name(dir) == ".reqfile")
        .then(|| paths::parent(dir))
}

/// A file whose name looks like a config but is not exactly `.reqfile/config.yaml`.
pub fn is_near_miss(path: &str) -> bool {
    let name = paths::file_name(path);
    paths::file_name(paths::parent(path)) == ".reqfile"
        && name != FILE_NAME
        && matches!(name.to_lowercase().as_str(), "config.yaml" | "config.yml")
}

/// The config file of folder `dir`.
pub fn path(dir: &str) -> String {
    paths::join(dir, &format!(".reqfile/{FILE_NAME}"))
}

/// How decision checks call Jev.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DecisionConfig {
    pub model: String,
    pub api_key_env: String,
    pub endpoint: String,
    pub concurrency: usize,
}

/// One config file, every key optional.
pub struct FolderConfig {
    dir: String,
    base: Option<String>,
    exclude: Option<FileGlob>,
    model: Option<String>,
    api_key_env: Option<String>,
    endpoint: Option<String>,
    concurrency: Option<usize>,
}

pub fn parse(path: &str, text: &str) -> Result<FolderConfig, ConfigError> {
    let dir = config_dir(path).expect("a config path").to_string();
    let root = yaml::parse(path, text)?;
    let mut config = FolderConfig {
        dir,
        base: None,
        exclude: None,
        model: None,
        api_key_env: None,
        endpoint: None,
        concurrency: None,
    };
    if matches!(root.value, Value::Null) {
        return Ok(config);
    }
    let mut fields = root.fields(path, "the config", &["base", "exclude", "decision"])?;
    config.base = fields
        .optional("base")
        .map(|n| n.text(path, "base"))
        .transpose()?;
    if let Some(node) = fields.optional("exclude") {
        let line = node.line;
        let patterns = node
            .list(path, "exclude")?
            .into_iter()
            .map(|n| n.text(path, "exclude"))
            .collect::<Result<Vec<_>, _>>()?;
        let patterns: Vec<&str> = patterns.iter().map(String::as_str).collect();
        config.exclude =
            Some(FileGlob::any(&patterns).map_err(|e| {
                ConfigError::at(path, line, format!("invalid `exclude` glob: {e}"))
            })?);
    }
    if let Some(node) = fields.optional("decision") {
        let mut decision = node.fields(
            path,
            "decision",
            &["model", "api_key_env", "endpoint", "concurrency"],
        )?;
        let text = |field: &mut yaml::Fields, key: &str| {
            field.optional(key).map(|n| n.text(path, key)).transpose()
        };
        config.model = text(&mut decision, "model")?;
        config.api_key_env = text(&mut decision, "api_key_env")?;
        config.endpoint = text(&mut decision, "endpoint")?;
        if let Some(node) = decision.optional("concurrency") {
            let line = node.line;
            match node.integer(path, "concurrency")? {
                n if n >= 1 => config.concurrency = Some(n as usize),
                _ => {
                    return Err(ConfigError::at(
                        path,
                        line,
                        "`concurrency` must be at least 1",
                    ));
                }
            }
        }
    }
    Ok(config)
}

/// Every config of the repository, resolved per folder.
#[derive(Default)]
pub struct Settings {
    /// Shallowest first; configs inside excluded folders are left out.
    configs: Vec<FolderConfig>,
}

impl Settings {
    /// Adds a config; configs must come shallowest first, and a config
    /// inside a folder excluded by an earlier one must be left out.
    pub fn add(&mut self, config: FolderConfig) {
        debug_assert!(
            self.configs
                .last()
                .is_none_or(|last| depth(&last.dir) <= depth(&config.dir))
        );
        debug_assert!(self.excluded_by(&path(&config.dir)).is_none());
        self.configs.push(config);
    }

    /// Whether a repository path is excluded from targets and from the search for Reqfiles.
    pub fn is_excluded(&self, path: &str) -> bool {
        path.split('/').any(|part| part == ".reqfile") || self.excluded_by(path).is_some()
    }

    /// The config whose `exclude` globs, relative to its folder, match `path`.
    pub fn excluded_by(&self, path: &str) -> Option<String> {
        self.configs.iter().find_map(|config| {
            let relative = paths::relative_to(&config.dir, path)?;
            let glob = config.exclude.as_ref()?;
            glob.is_match(relative).then(|| self::path(&config.dir))
        })
    }

    /// The `base` of `--changed` for files of folder `dir`.
    pub fn base(&self, dir: &str) -> Option<&str> {
        self.nearest(dir, |c| c.base.as_deref())
    }

    /// How decision checks of a Reqfile in folder `dir` call Jev.
    pub fn decision(&self, dir: &str) -> DecisionConfig {
        let text = |key: fn(&FolderConfig) -> Option<&str>, default: &str| {
            self.nearest(dir, key).unwrap_or(default).to_string()
        };
        DecisionConfig {
            model: text(|c| c.model.as_deref(), DEFAULT_MODEL),
            api_key_env: text(|c| c.api_key_env.as_deref(), DEFAULT_API_KEY_ENV),
            endpoint: text(|c| c.endpoint.as_deref(), DEFAULT_ENDPOINT),
            concurrency: self
                .nearest(dir, |c| c.concurrency)
                .unwrap_or(DEFAULT_CONCURRENCY),
        }
    }

    /// A key from the config of `dir` or, if unset there, of its nearest ancestor setting it.
    fn nearest<'s, T>(
        &'s self,
        dir: &str,
        key: impl Fn(&'s FolderConfig) -> Option<T>,
    ) -> Option<T> {
        self.configs
            .iter()
            .rev()
            .filter(|c| paths::contains_dir(&c.dir, dir))
            .find_map(key)
    }
}

/// How deep a folder is, the root being 0.
pub fn depth(dir: &str) -> usize {
    if dir.is_empty() {
        0
    } else {
        dir.split('/').count()
    }
}
