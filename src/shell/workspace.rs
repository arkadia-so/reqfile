//! The repository reqfile works on: its root, settings, target files and Reqfiles.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::git;
use crate::core::config::{self, Settings};
use crate::core::paths;
use crate::core::plan;
use crate::core::reqfile::{self, Reqfile};

pub struct Workspace {
    pub root: PathBuf,
    pub settings: Settings,
    /// Existing, non-excluded files, as repository paths.
    pub targets: BTreeSet<String>,
    /// Sorted by path, with ids unique across the repository.
    pub reqfiles: Vec<Reqfile>,
}

impl Workspace {
    /// Loads the repository containing `cwd`, or every error found on the way.
    pub fn load(cwd: &Path) -> Result<Self, Vec<String>> {
        let root = git::root(cwd).map_err(|e| vec![e])?;
        // A file renamed but not yet staged is still listed under its old name.
        let files: Vec<String> = git::files(&root)
            .map_err(|e| vec![e])?
            .into_iter()
            // Regular files only: a symlink could point outside the repository,
            // and its content would be handed to tools and sent to Jev.
            .filter(|p| fs::symlink_metadata(root.join(p)).is_ok_and(|m| m.is_file()))
            .collect();
        let mut errors = Vec::new();
        let settings = load_settings(&root, &files, &mut errors);
        let targets: BTreeSet<String> = files
            .into_iter()
            .filter(|p| !settings.is_excluded(p))
            .collect();
        let reqfiles = load_reqfiles(&root, &targets, &mut errors);
        if errors.is_empty() {
            Ok(Self {
                root,
                settings,
                targets,
                reqfiles,
            })
        } else {
            Err(errors)
        }
    }

    /// `path` (relative to `cwd`) as a repository path.
    pub fn repository_path(&self, cwd: &Path, path: &Path) -> Result<String, String> {
        let cwd = cwd
            .canonicalize()
            .map_err(|e| format!("cannot resolve the current folder: {e}"))?;
        let absolute = cwd.join(path);
        let relative = absolute
            .strip_prefix(&self.root)
            .map_err(|_| format!("{} is outside the repository", path.display()))?;
        paths::normalize(&relative.to_string_lossy())
            .ok_or_else(|| format!("{} is outside the repository", path.display()))
    }
}

/// Every `.reqfile/config.yaml` among `files`, read shallowest first so a
/// config in a folder a parent excludes is never read.
fn load_settings(root: &Path, files: &[String], errors: &mut Vec<String>) -> Settings {
    let mut configs: Vec<(&str, &String)> = files
        .iter()
        .filter_map(|p| config::config_dir(p).map(|dir| (dir, p)))
        .collect();
    configs.sort_by_key(|(dir, _)| config::depth(dir));
    let mut settings = Settings::default();
    for (_, path) in configs {
        if settings.excluded_by(path).is_some() {
            continue;
        }
        match read(root, path)
            .and_then(|text| config::parse(path, &text).map_err(|e| e.to_string()))
        {
            Ok(config) => settings.add(config),
            Err(e) => errors.push(e),
        }
    }
    for path in files
        .iter()
        .filter(|p| config::is_near_miss(p) && settings.excluded_by(p).is_none())
    {
        errors.push(format!(
            "{path}:1: a config file must be named exactly .reqfile/config.yaml"
        ));
    }
    settings
}

/// Every Reqfile among the targets, sorted by path, with unique ids.
fn load_reqfiles(
    root: &Path,
    targets: &BTreeSet<String>,
    errors: &mut Vec<String>,
) -> Vec<Reqfile> {
    for path in targets.iter().filter(|p| reqfile::is_near_miss(p)) {
        match read(root, path) {
            Ok(text) if !reqfile::looks_like_reqfile(&text) => {}
            Ok(_) => errors.push(format!(
                "{path}:1: a requirements file must be named exactly {}",
                reqfile::FILE_NAME
            )),
            Err(e) => errors.push(e),
        }
    }
    let mut reqfiles = Vec::new();
    for path in targets
        .iter()
        .filter(|p| paths::file_name(p) == reqfile::FILE_NAME)
    {
        match read(root, path)
            .and_then(|text| reqfile::parse(path, &text).map_err(|e| e.to_string()))
        {
            Ok(reqfile) => reqfiles.push(reqfile),
            Err(e) => errors.push(e),
        }
    }
    errors.extend(
        plan::check_unique_ids(&reqfiles)
            .iter()
            .map(ToString::to_string),
    );
    reqfiles
}

fn read(root: &Path, path: &str) -> Result<String, String> {
    fs::read_to_string(root.join(path)).map_err(|e| format!("{path}: cannot read: {e}"))
}
