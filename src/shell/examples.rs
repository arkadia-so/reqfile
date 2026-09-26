//! `reqfile test`: runs each requirement's checks on its labeled examples,
//! each case alone in a fresh repository.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::check;
use super::workspace::Workspace;
use crate::core::config::DecisionConfig;
use crate::core::examples::{self, Outcome, Tested};
use crate::core::report::Report;
use crate::core::reqfile::{Check, Reqfile, Requirement};

/// The report and exit code of `reqfile test`, or the errors that prevent it.
pub fn run(cwd: &Path, only: Option<&[String]>) -> Result<(String, i32), Vec<String>> {
    let workspace = Workspace::load(cwd)?;
    let requirements: Vec<(&Reqfile, &Requirement)> = workspace
        .reqfiles
        .iter()
        .flat_map(|r| r.requirements.iter().map(move |req| (r, req)))
        .filter(|(_, req)| only.is_none_or(|ids| ids.contains(&req.id)))
        .collect();
    if let Some(ids) = only {
        let unknown: Vec<String> = ids
            .iter()
            .filter(|id| !requirements.iter().any(|(_, r)| &r.id == *id))
            .map(|id| format!("--only: no requirement has the id {id}"))
            .collect();
        if !unknown.is_empty() {
            return Err(unknown);
        }
    }
    let mut tested = Vec::new();
    for (reqfile, requirement) in requirements {
        let own = workspace
            .root
            .join(&reqfile.dir)
            .join(".reqfile")
            .join(&requirement.id);
        let cases_dir = own.join("examples");
        if !cases_dir.is_dir() {
            continue;
        }
        let mut cases = Vec::new();
        for (name, case) in sorted_dirs(&cases_dir)? {
            let Some(label) = examples::label(&name) else {
                return Err(vec![format!(
                    "{}: an example folder must be named violation-… or ok-…",
                    case.display()
                )]);
            };
            let jev = workspace.settings.decision(&reqfile.dir);
            let outcome = run_case(
                &workspace.root.join(&reqfile.path),
                &own,
                &case,
                requirement,
                &jev,
            )
            .map(|report| examples::outcome(label, &requirement.id, &report))
            .unwrap_or_else(Outcome::Error);
            cases.push((name, label, outcome));
        }
        tested.push(Tested {
            id: requirement.id.clone(),
            measured: requirement
                .checks
                .iter()
                .any(|c| matches!(c, Check::Decision(_))),
            cases,
        });
    }
    Ok(examples::render(&tested))
}

/// Runs a requirement's checks on one case: a fresh repository holding the
/// case's files, the Reqfile, the requirement's `.reqfile/<ID>/` and the Jev
/// settings of its folder.
fn run_case(
    reqfile: &Path,
    own: &Path,
    case: &Path,
    requirement: &Requirement,
    jev: &DecisionConfig,
) -> Result<Report, String> {
    let repo = TempRepo::new()?;
    copy_tree(case, repo.path(), None)?;
    fs::copy(reqfile, repo.path().join("Reqfile.yaml")).map_err(|e| e.to_string())?;
    let own_copy = repo.path().join(".reqfile").join(&requirement.id);
    copy_tree(own, &own_copy, Some("examples"))?;
    let quote = |s: &str| serde_json::Value::from(s).to_string();
    let config = format!(
        "decision:\n  model: {}\n  api_key_env: {}\n  endpoint: {}\n  concurrency: {}\n",
        quote(&jev.model),
        quote(&jev.api_key_env),
        quote(&jev.endpoint),
        jev.concurrency
    );
    fs::write(repo.path().join(".reqfile").join("config.yaml"), config)
        .map_err(|e| e.to_string())?;
    let options = check::Options {
        changed: None,
        only: Some(vec![requirement.id.clone()]),
        fast: false,
        log: None,
        log_tags: Vec::new(),
    };
    Ok(check::run(repo.path(), &options))
}

/// The subfolders of `dir`, with their names, sorted by name.
fn sorted_dirs(dir: &Path) -> Result<Vec<(String, PathBuf)>, Vec<String>> {
    let fail = |e: std::io::Error| vec![format!("{}: {e}", dir.display())];
    let mut dirs = Vec::new();
    for entry in fs::read_dir(dir).map_err(fail)? {
        let entry = entry.map_err(fail)?;
        if entry.file_type().map_err(fail)?.is_dir() {
            dirs.push((
                entry.file_name().to_string_lossy().into_owned(),
                entry.path(),
            ));
        }
    }
    dirs.sort();
    Ok(dirs)
}

/// Copies a folder, leaving out the entry named `skip` at its top level.
fn copy_tree(from: &Path, to: &Path, skip: Option<&str>) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("cannot copy {}: {e}", from.display());
    fs::create_dir_all(to).map_err(fail)?;
    for entry in fs::read_dir(from).map_err(fail)? {
        let entry = entry.map_err(fail)?;
        if Some(entry.file_name().to_string_lossy().as_ref()) == skip {
            continue;
        }
        let target = to.join(entry.file_name());
        let kind = entry.file_type().map_err(fail)?;
        if kind.is_symlink() {
            return Err(format!(
                "{} is a symlink; examples hold regular files only",
                entry.path().display()
            ));
        } else if kind.is_dir() {
            copy_tree(&entry.path(), &target, None)?;
        } else {
            fs::copy(entry.path(), &target).map_err(fail)?;
        }
    }
    Ok(())
}

/// A git repository in a temporary folder, removed when dropped. The folder
/// is created exclusively, with a random name and owner-only permissions, so
/// nothing another user placed in the temporary folder can redirect copies.
struct TempRepo(tempfile::TempDir);

impl TempRepo {
    fn new() -> Result<Self, String> {
        let dir = tempfile::Builder::new()
            .prefix("reqfile-example-")
            .tempdir()
            .map_err(|e| format!("cannot create a temporary folder: {e}"))?;
        let status = Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .status()
            .map_err(|e| format!("could not run git: {e}"))?;
        if !status.success() {
            return Err("git init failed for an example".into());
        }
        Ok(Self(dir))
    }

    fn path(&self) -> &Path {
        self.0.path()
    }
}
