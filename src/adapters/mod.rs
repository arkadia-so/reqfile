//! Reusable checks, composed before execution. The runner knows manifests and
//! provenance; language and library semantics stay in ordinary check programs.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value as Json};
use sha2::{Digest, Sha256};

use crate::core::error::ConfigError;
use crate::core::paths;
use crate::core::reqfile::{self, Check, GitRef, Location};
use crate::core::resolve::{Assets, Effective, Source};
use crate::core::yaml::{self, Node, Value};

pub const FILE_NAME: &str = "Adapter.yaml";

#[derive(Debug, Clone)]
pub struct Reference {
    pub location: Location,
    pub path: String,
    pub check: String,
    pub binding: Map<String, Json>,
    pub line: usize,
}

/// Attached to an executable leaf, not to its requirement: two checks can
/// come from different packages and carry different project bindings.
#[derive(Debug, Clone)]
pub struct Context {
    pub assets: Assets,
    pub origins: Vec<String>,
    pub binding: Map<String, Json>,
    pub fingerprint: String,
}

pub fn reference(path: &str, node: Node) -> Result<Reference, ConfigError> {
    let line = node.line;
    let mut fields = node.fields(
        path,
        "an adapter reference",
        &["use", "path", "check", "with"],
    )?;
    let location = fields.required("use")?.text(path, "use")?;
    let location =
        reqfile::parse_location(&location).map_err(|e| ConfigError::at(path, line, e))?;
    if matches!(
        &location,
        Location::Git {
            reference: GitRef::Tag(_),
            ..
        }
    ) {
        return Err(ConfigError::at(
            path,
            line,
            "an adapter must be pinned to a full commit id, never a movable tag",
        ));
    }
    let folder = fields
        .optional("path")
        .map(|n| n.text(path, "path"))
        .transpose()?
        .unwrap_or_else(|| ".".into());
    if folder.starts_with('/') || paths::normalize(&folder).is_none() {
        return Err(ConfigError::at(
            path,
            line,
            "adapter `path` must stay inside its package",
        ));
    }
    if !matches!(location, Location::Git { .. }) && folder != "." {
        return Err(ConfigError::at(
            path,
            line,
            "`path` is only for a Git adapter; a local `use` already names its folder",
        ));
    }
    let check = fields.required("check")?.text(path, "check")?;
    let binding = match fields.optional("with") {
        None => Map::new(),
        Some(n) => {
            if !matches!(n.value, Value::Map(_)) {
                return Err(ConfigError::at(
                    path,
                    n.line,
                    "adapter `with` must be a mapping",
                ));
            }
            n.into_json().as_object().expect("a mapping").clone()
        }
    };
    Ok(Reference {
        location,
        path: folder,
        check,
        binding,
        line,
    })
}

struct Manifest {
    kind: String,
    required: Vec<String>,
    optional: Vec<String>,
    exports: BTreeMap<String, Vec<Check>>,
}

fn manifest(path: &str, text: &str) -> Result<Manifest, ConfigError> {
    let mut fields = yaml::parse(path, text)?.fields(
        path,
        "Adapter.yaml",
        &["adapter", "kind", "inputs", "checks"],
    )?;
    let version = fields.required("adapter")?;
    let line = version.line;
    let version = version.integer(path, "adapter")?;
    if version != 1 {
        return Err(ConfigError::at(
            path,
            line,
            format!("unsupported adapter version {version}; expected 1"),
        ));
    }
    let kind = fields.required("kind")?;
    let line = kind.line;
    let kind = kind.text(path, "kind")?;
    if !["language", "library", "project"].contains(&kind.as_str()) {
        return Err(ConfigError::at(
            path,
            line,
            "adapter kind must be language, library or project",
        ));
    }
    let mut required = Vec::new();
    let mut optional = Vec::new();
    if let Some(inputs) = fields.optional("inputs") {
        let mut inputs = inputs.fields(path, "adapter inputs", &["required", "optional"])?;
        for (name, target) in [("required", &mut required), ("optional", &mut optional)] {
            if let Some(node) = inputs.optional(name) {
                for n in node.list(path, name)? {
                    let line = n.line;
                    let name = n.text(path, "input name")?;
                    if target.contains(&name) {
                        return Err(ConfigError::at(
                            path,
                            line,
                            format!("duplicate input `{name}`"),
                        ));
                    }
                    target.push(name);
                }
            }
        }
        if let Some(name) = required.iter().find(|n| optional.contains(n)) {
            return Err(ConfigError::at(
                path,
                inputs.line,
                format!("input `{name}` is both required and optional"),
            ));
        }
    }
    let checks = fields.required("checks")?;
    let line = checks.line;
    let Value::Map(entries) = checks.value else {
        return Err(ConfigError::at(
            path,
            line,
            "adapter `checks` must map export names to check lists",
        ));
    };
    let mut exports = BTreeMap::new();
    for (key, node) in entries {
        if key.name.trim().is_empty() {
            return Err(ConfigError::at(
                path,
                key.line,
                "an adapter export needs a name",
            ));
        }
        exports.insert(
            key.name.clone(),
            reqfile::checks(path, &key.name, node)?.list,
        );
    }
    if exports.is_empty() {
        return Err(ConfigError::at(
            path,
            line,
            "an adapter must export at least one check",
        ));
    }
    Ok(Manifest {
        kind,
        required,
        optional,
        exports,
    })
}

#[derive(Clone)]
enum Package {
    Repo,
    External { root: PathBuf, label: String },
}

impl Package {
    fn label(&self, path: &str) -> String {
        match self {
            Self::Repo => path.to_string(),
            Self::External { label, .. } => format!("{label}:{path}"),
        }
    }

    fn assets(&self, dir: &str) -> Assets {
        match self {
            Self::Repo => Assets::Repo(dir.to_string()),
            Self::External { root, .. } => Assets::External(root.join(dir)),
        }
    }
}

/// All reads, including historical reads, go through the caller for the
/// adopting repository. Git packages are immutable, already fetched roots.
type ReadRepository<'a> = &'a dyn Fn(&str) -> Result<Vec<u8>, String>;
type Fetch<'a> = &'a dyn Fn(&str, &GitRef) -> Result<PathBuf, String>;

struct Resolver<'a> {
    files: &'a [String],
    read: ReadRepository<'a>,
    fetch: Fetch<'a>,
}

pub fn expand(
    blocks: &mut [Effective],
    files: &[String],
    read: ReadRepository<'_>,
    sources: &[Source],
    fetch: Fetch<'_>,
) -> Result<(), Vec<String>> {
    let resolver = Resolver { files, read, fetch };
    let mut errors = Vec::new();
    for block in blocks {
        let (package, dir) = match &block.imported {
            Some(i) if i.checks_inherited => match &i.location {
                Location::Git { repo, reference } => {
                    let source = sources
                        .iter()
                        .find(|s| &s.repo == repo && &s.reference == reference)
                        .expect("requirement source loaded");
                    let definition = i
                        .definition
                        .rsplit_once(':')
                        .map_or(i.definition.as_str(), |(p, _)| p);
                    (
                        Package::External {
                            root: source.root.clone(),
                            label: i.location.describe(),
                        },
                        paths::parent(definition).to_string(),
                    )
                }
                Location::Local(_) => {
                    let definition = i
                        .definition
                        .rsplit_once(':')
                        .map_or(i.definition.as_str(), |(p, _)| p);
                    (Package::Repo, paths::parent(definition).to_string())
                }
            },
            _ => (Package::Repo, block.dir.clone()),
        };
        let mut expanded = Vec::new();
        for check in &block.checks {
            match resolver.check(
                check,
                &package,
                &dir,
                &Map::new(),
                &[],
                &[],
                &mut Vec::new(),
                &mut expanded,
            ) {
                Ok(()) => {}
                Err(e) => errors.push(format!("{}: requirement {}: {e}", block.block(), block.id)),
            }
        }
        // The decision protocol identifies questions by requirement id. Keep
        // its existing one-question invariant instead of silently overwriting.
        if expanded
            .iter()
            .filter(|c| matches!(c, Check::Decision(_)))
            .count()
            > 1
        {
            errors.push(format!("{}: requirement {} composes more than one decision check; combine its unit selectors into one decision", block.block(), block.id));
        }
        block.checks = expanded;
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

impl Resolver<'_> {
    fn read_file(&self, package: &Package, path: &str) -> Result<Vec<u8>, String> {
        match package {
            Package::Repo => {
                if !self.files.iter().any(|p| p == path) {
                    return Err(format!(
                        "{path}: not a regular, non-ignored file of the repository"
                    ));
                }
                (self.read)(path)
            }
            Package::External { root, .. } => {
                regular_path(root, path)?;
                fs::read(root.join(path)).map_err(|e| format!("{}: {e}", package.label(path)))
            }
        }
    }

    fn digest(&self, package: &Package, dir: &str) -> Result<String, String> {
        let files = match package {
            Package::Repo => self
                .files
                .iter()
                .filter(|p| paths::relative_to(dir, p).is_some())
                .cloned()
                .collect(),
            Package::External { root, .. } => external_files(root, dir)?,
        };
        let mut hash = Sha256::new();
        for path in files {
            let relative = paths::relative_to(dir, &path).expect("in adapter folder");
            hash.update(relative.as_bytes());
            hash.update([0]);
            hash.update(self.read_file(package, &path)?);
            hash.update([0]);
        }
        Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
    }

    #[allow(clippy::too_many_arguments)]
    fn check(
        &self,
        check: &Check,
        package: &Package,
        dir: &str,
        inherited: &Map<String, Json>,
        origins: &[String],
        fingerprints: &[String],
        stack: &mut Vec<String>,
        out: &mut Vec<Check>,
    ) -> Result<(), String> {
        let Check::Adapter(reference) = check else {
            let mut leaf = check.clone();
            if !origins.is_empty() {
                let context = Context {
                    assets: package.assets(dir),
                    origins: origins.to_vec(),
                    binding: inherited.clone(),
                    fingerprint: crate::core::runlog::fingerprint(&format!(
                        "{fingerprints:?}\n{}",
                        Json::Object(inherited.clone())
                    )),
                };
                match &mut leaf {
                    Check::Command(c) => c.context = Some(context),
                    Check::Decision(c) => c.context = Some(context),
                    Check::Adapter(_) => unreachable!(),
                }
            }
            out.push(leaf);
            return Ok(());
        };
        if stack.len() >= 64 {
            return Err("adapter composition exceeds 64 levels".into());
        }
        let (package, folder) = match &reference.location {
            Location::Local(path) => {
                let folder = paths::normalize(&paths::join(dir, path)).ok_or_else(|| {
                    format!(
                        "{}:{}: adapter `use` escapes its package",
                        package.label(dir),
                        reference.line
                    )
                })?;
                (package.clone(), folder)
            }
            Location::Git {
                repo,
                reference: pin,
            } => {
                let root = (self.fetch)(repo, pin)?;
                (
                    Package::External {
                        root,
                        label: reference.location.describe(),
                    },
                    paths::normalize(&reference.path).expect("validated path"),
                )
            }
        };
        let file = paths::join(&folder, FILE_NAME);
        let shown = package.label(&file);
        let identity = format!("{shown}#{}", reference.check);
        if stack.contains(&identity) {
            return Err(format!(
                "adapter cycle: {} -> {identity}",
                stack.join(" -> ")
            ));
        }
        let text = String::from_utf8(self.read_file(&package, &file)?)
            .map_err(|_| format!("{shown}: not UTF-8"))?;
        let manifest = manifest(&shown, &text).map_err(|e| e.to_string())?;
        let allowed =
            |name: &String| manifest.required.contains(name) || manifest.optional.contains(name);
        if let Some(name) = reference.binding.keys().find(|n| !allowed(n)) {
            return Err(format!(
                "{shown}: unknown adapter input `{name}` for {}",
                reference.check
            ));
        }
        let mut binding: Map<String, Json> = inherited
            .iter()
            .filter(|(n, _)| allowed(n))
            .map(|(n, v)| (n.clone(), v.clone()))
            .collect();
        binding.extend(reference.binding.clone());
        if let Some(name) = manifest.required.iter().find(|n| !binding.contains_key(*n)) {
            return Err(format!(
                "{shown}: missing required adapter input `{name}` for {}",
                reference.check
            ));
        }
        let checks = manifest.exports.get(&reference.check).ok_or_else(|| {
            format!(
                "{shown}: no check export `{}` (available: {})",
                reference.check,
                manifest
                    .exports
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
        let mut origins = origins.to_vec();
        origins.push(format!("{} {identity}", manifest.kind));
        let mut fingerprints = fingerprints.to_vec();
        fingerprints.push(format!("{identity}:{}", self.digest(&package, &folder)?));
        stack.push(identity);
        for check in checks {
            self.check(
                check,
                &package,
                &folder,
                &binding,
                &origins,
                &fingerprints,
                stack,
                out,
            )?;
        }
        stack.pop();
        Ok(())
    }
}

fn regular_path(root: &Path, path: &str) -> Result<(), String> {
    let mut current = root.to_path_buf();
    for component in Path::new(path).components() {
        current.push(component);
        let meta =
            fs::symlink_metadata(&current).map_err(|e| format!("{}: {e}", current.display()))?;
        if meta.is_symlink() {
            return Err(format!(
                "{}: adapter assets cannot be symlinks",
                current.display()
            ));
        }
    }
    Ok(())
}

fn external_files(root: &Path, dir: &str) -> Result<Vec<String>, String> {
    regular_path(root, dir)?;
    let mut pending = vec![root.join(dir)];
    let mut files = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                return Err(format!(
                    "{}: adapter assets cannot be symlinks",
                    entry.path().display()
                ));
            }
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                files.push(
                    entry
                        .path()
                        .strip_prefix(root)
                        .expect("under root")
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    files.sort();
    Ok(files)
}
