//! Which requirements apply where, and which files each check runs on.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::config;
use super::decision::DecisionSpec;
use super::error::ConfigError;
use super::paths;
use super::reqfile::{CommandCheck, Reqfile, Requirement};

/// Checks that requirement ids are unique across the repository.
pub fn check_unique_ids(reqfiles: &[Reqfile]) -> Vec<ConfigError> {
    let mut seen: HashMap<&str, &Reqfile> = HashMap::new();
    let mut errors = Vec::new();
    for reqfile in reqfiles {
        for requirement in &reqfile.requirements {
            match seen.get(requirement.id.as_str()) {
                Some(first) => errors.push(ConfigError::at(
                    &reqfile.path,
                    requirement.line,
                    format!(
                        "duplicate requirement id {}, also defined in {}",
                        requirement.id, first.path
                    ),
                )),
                None => {
                    seen.insert(&requirement.id, reqfile);
                }
            }
        }
    }
    errors
}

/// The requirements that apply to files of folder `dir`: those of every
/// Reqfile in that folder and its ancestors, the root first.
pub fn applicable<'r>(reqfiles: &'r [Reqfile], dir: &str) -> Vec<(&'r Reqfile, &'r Requirement)> {
    let mut found: Vec<&Reqfile> = reqfiles
        .iter()
        .filter(|r| paths::contains_dir(&r.dir, dir))
        .collect();
    found.sort_by_key(|r| r.dir.matches('/').count() + usize::from(!r.dir.is_empty()));
    found
        .into_iter()
        .flat_map(|r| r.requirements.iter().map(move |req| (r, req)))
        .collect()
}

/// The files checks may target.
pub struct Selection {
    /// Existing, non-excluded files of the repository.
    pub targets: BTreeSet<String>,
    /// With `--changed`: for each Reqfile folder, what changed since the
    /// base that applies to it.
    pub changed: Option<BTreeMap<String, Changed>>,
}

/// Paths changed since a base, deleted ones included.
#[derive(Clone)]
pub struct Changed {
    /// Changed paths that checks may target.
    pub targets: BTreeSet<String>,
    /// Every changed path, including excluded ones such as `.reqfile/` files.
    pub all: BTreeSet<String>,
}

impl Selection {
    /// Candidate paths under `dir`, with their path relative to `dir`. With
    /// `full`, every target plus the changed paths, so a widened requirement
    /// still sees deletions.
    fn candidates<'s>(
        &'s self,
        dir: &'s str,
        full: bool,
    ) -> impl Iterator<Item = (&'s str, &'s str)> + 's {
        let changed = self.changed.as_ref().map(|by_dir| {
            &by_dir
                .get(dir)
                .expect("changed paths are listed for every Reqfile folder")
                .targets
        });
        let (first, second) = match changed {
            Some(changed) if !full => (changed, None),
            Some(changed) => (&self.targets, Some(changed)),
            None => (&self.targets, None),
        };
        first
            .iter()
            .chain(
                second
                    .into_iter()
                    .flatten()
                    .filter(|p| !self.targets.contains(*p)),
            )
            .filter_map(move |path| paths::relative_to(dir, path).map(|rel| (path.as_str(), rel)))
    }

    fn exists(&self, path: &str) -> bool {
        self.targets.contains(path)
    }

    /// Whether a requirement's definition changed with `--changed`: its
    /// Reqfile, its `.reqfile/<ID>/` files, or a config above its folder
    /// (settings) or below it (exclusions within its scope). Such a
    /// requirement is checked on its whole scope, since unchanged files may
    /// now fail it.
    pub fn redefined(&self, reqfile: &Reqfile, requirement: &Requirement) -> bool {
        let Some(changed) = self
            .changed
            .as_ref()
            .and_then(|by_dir| by_dir.get(&reqfile.dir))
        else {
            return false;
        };
        let own = paths::join(&reqfile.dir, &format!(".reqfile/{}/", requirement.id));
        changed.all.iter().any(|path| {
            path == &reqfile.path
                || path.starts_with(&own)
                || config::config_dir(path).is_some_and(|dir| {
                    paths::contains_dir(dir, &reqfile.dir) || paths::contains_dir(&reqfile.dir, dir)
                })
        })
    }
}

#[derive(Debug, PartialEq)]
pub enum CommandPlan {
    /// Run with these arguments, relative to the folder of the Reqfile.
    Run(Vec<String>),
    NoMatchingFiles,
}

/// With `full`, the command sees its whole scope rather than changed files.
pub fn plan_command(
    dir: &str,
    check: &CommandCheck,
    selection: &Selection,
    full: bool,
) -> CommandPlan {
    let Some(glob) = &check.files else {
        return CommandPlan::Run(Vec::new());
    };
    let matching: Vec<(&str, &str)> = selection
        .candidates(dir, full)
        .filter(|(_, rel)| glob.is_match(rel))
        .collect();
    if check.pass_files {
        // A deleted file cannot be handed to a tool; it only triggers commands that do not take files.
        let args: Vec<String> = matching
            .iter()
            .filter(|(path, _)| selection.exists(path))
            .map(|(_, rel)| rel.to_string())
            .collect();
        if args.is_empty() {
            CommandPlan::NoMatchingFiles
        } else {
            CommandPlan::Run(args)
        }
    } else if matching.is_empty() {
        CommandPlan::NoMatchingFiles
    } else {
        CommandPlan::Run(Vec::new())
    }
}

/// The existing files a decision check extracts units from: repository
/// path and path relative to the folder of the Reqfile.
pub fn decision_files<'s>(
    dir: &'s str,
    spec: &DecisionSpec,
    selection: &'s Selection,
    full: bool,
) -> Vec<(&'s str, &'s str)> {
    selection
        .candidates(dir, full)
        .filter(|(path, rel)| selection.exists(path) && spec.applies_to(rel))
        .collect()
}
