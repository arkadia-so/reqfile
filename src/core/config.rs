//! Settings: `.reqfile/config.yaml` in any folder, applying to that folder
//! and everything under it. Nothing is specific to the repository root: for
//! each key the nearest config wins, and exclusions add up down the tree.

use super::error::ConfigError;
use super::paths;
use super::reqfile::FileGlob;
use super::yaml::{self, Value};

const FILE_NAME: &str = "config.yaml";
pub const OPENROUTER_API_KEY_ENV: &str = "OPENROUTER_API_KEY";
pub const TYPESAFE_API_KEY_ENV: &str = "TYPESAFE_API_KEY";
const DEFAULT_CONCURRENCY: usize = 16;

/// Who serves Jev when no config names an endpoint or a key variable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Provider {
    #[default]
    OpenRouter,
    /// TypeSafe's own API, which makes Jev.
    TypeSafe,
}

impl Provider {
    /// The provider whose key is set, OpenRouter first; OpenRouter when
    /// neither is, so the missing key is named.
    pub fn from_keys(is_set: impl Fn(&str) -> bool) -> Self {
        if !is_set(OPENROUTER_API_KEY_ENV) && is_set(TYPESAFE_API_KEY_ENV) {
            Provider::TypeSafe
        } else {
            Provider::OpenRouter
        }
    }

    /// Its model, key variable and endpoint. The model is Jev's latest
    /// release, so decision checks follow new versions without config changes.
    fn defaults(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Provider::OpenRouter => (
                "~typesafe/jev-latest",
                OPENROUTER_API_KEY_ENV,
                "https://openrouter.ai/api/v1",
            ),
            Provider::TypeSafe => (
                "jev-latest",
                TYPESAFE_API_KEY_ENV,
                "https://api.typesafe.ai/v1",
            ),
        }
    }
}

/// What to add to "`variable` is not set" so people know the other way to
/// reach Jev.
pub fn other_key(variable: &str) -> &'static str {
    if variable == OPENROUTER_API_KEY_ENV {
        " (or set TYPESAFE_API_KEY to call TypeSafe's API directly)"
    } else {
        ""
    }
}

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

impl DecisionConfig {
    /// Whether the model names one exact version, rather than an alias such
    /// as `~typesafe/jev-latest` that moves to new releases.
    pub fn model_is_pinned(&self) -> bool {
        !self.model.starts_with('~') && !self.model.ends_with("latest")
    }
}

/// One config file, every key optional.
pub struct FolderConfig {
    dir: String,
    base: Option<String>,
    exclude: Option<FileGlob>,
    /// The `exclude` globs as written, which identify them.
    exclude_patterns: Vec<String>,
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
        exclude_patterns: Vec::new(),
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
        let globs: Vec<&str> = patterns.iter().map(String::as_str).collect();
        config.exclude =
            Some(FileGlob::any(&globs).map_err(|e| {
                ConfigError::at(path, line, format!("invalid `exclude` glob: {e}"))
            })?);
        config.exclude_patterns = patterns;
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
    /// Who serves Jev where no config names an endpoint or a key variable.
    provider: Provider,
}

impl Settings {
    pub fn set_provider(&mut self, provider: Provider) {
        self.provider = provider;
    }

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
        // A config naming where to call Jev keeps OpenRouter's defaults for
        // the rest, whatever key the environment has.
        let configured = self.nearest(dir, |c| c.endpoint.as_deref()).is_some()
            || self.nearest(dir, |c| c.api_key_env.as_deref()).is_some();
        let provider = if configured {
            Provider::OpenRouter
        } else {
            self.provider
        };
        let (model, api_key_env, endpoint) = provider.defaults();
        DecisionConfig {
            model: text(|c| c.model.as_deref(), model),
            api_key_env: text(|c| c.api_key_env.as_deref(), api_key_env),
            endpoint: text(|c| c.endpoint.as_deref(), endpoint),
            concurrency: self
                .nearest(dir, |c| c.concurrency)
                .unwrap_or(DEFAULT_CONCURRENCY),
        }
    }

    /// Settings for a single folder, the root, with only Jev settings: how
    /// `reqfile test` runs a requirement's examples with the settings of its
    /// folder.
    pub fn with_decision(decision: &DecisionConfig) -> Self {
        Self {
            configs: vec![FolderConfig {
                dir: String::new(),
                base: None,
                exclude: None,
                exclude_patterns: Vec::new(),
                model: Some(decision.model.clone()),
                api_key_env: Some(decision.api_key_env.clone()),
                endpoint: Some(decision.endpoint.clone()),
                concurrency: Some(decision.concurrency),
            }],
            provider: Provider::default(),
        }
    }

    /// Identifies what the settings of folder `dir` change in the results
    /// of its checks: the exclusions of the configs at or above it and, with
    /// `decision`, how Jev is asked. `base` and `concurrency` change what
    /// runs or how fast, never a result.
    pub fn fingerprint(&self, dir: &str, decision: bool) -> String {
        let mut text = String::new();
        if decision {
            let jev = self.decision(dir);
            text += &format!("{}\n{}\n{}\n", jev.model, jev.api_key_env, jev.endpoint);
        }
        for config in self
            .configs
            .iter()
            .filter(|c| paths::contains_dir(&c.dir, dir))
        {
            text += &format!("{}:{:?}\n", config.dir, config.exclude_patterns);
        }
        super::runlog::fingerprint(&text)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(provider: Provider, config: &str) -> Settings {
        let mut settings = Settings::default();
        settings.set_provider(provider);
        settings.add(parse(".reqfile/config.yaml", config).expect("a valid config"));
        settings
    }

    #[test]
    fn openrouter_is_chosen_unless_only_the_typesafe_key_is_set() {
        let set = |keys: &'static [&'static str]| move |v: &str| keys.contains(&v);
        assert_eq!(Provider::from_keys(set(&[])), Provider::OpenRouter);
        assert_eq!(
            Provider::from_keys(set(&[OPENROUTER_API_KEY_ENV, TYPESAFE_API_KEY_ENV])),
            Provider::OpenRouter
        );
        assert_eq!(
            Provider::from_keys(set(&[TYPESAFE_API_KEY_ENV])),
            Provider::TypeSafe
        );
    }

    #[test]
    fn typesafe_defaults_call_its_api_with_its_key() {
        let jev = settings(Provider::TypeSafe, "").decision("");
        assert_eq!(jev.endpoint, "https://api.typesafe.ai/v1");
        assert_eq!(jev.api_key_env, TYPESAFE_API_KEY_ENV);
        assert_eq!(jev.model, "jev-latest");
    }

    #[test]
    fn a_config_naming_the_endpoint_keeps_openrouter_defaults() {
        let jev = settings(
            Provider::TypeSafe,
            "decision:\n  endpoint: http://localhost:1\n",
        )
        .decision("");
        assert_eq!(jev.endpoint, "http://localhost:1");
        assert_eq!(jev.api_key_env, OPENROUTER_API_KEY_ENV);
        assert_eq!(jev.model, "~typesafe/jev-latest");
    }

    #[test]
    fn the_provider_changes_the_fingerprint_so_answers_are_not_mixed() {
        assert_ne!(
            settings(Provider::OpenRouter, "").fingerprint("", true),
            settings(Provider::TypeSafe, "").fingerprint("", true)
        );
    }
}
