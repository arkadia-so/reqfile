//! `reqfile test`: runs each requirement's checks on its labeled examples,
//! each case alone in a fresh repository, its files reached only through
//! `$REQFILE_ASSETS`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::check;
use super::workspace::Workspace;
use crate::core::config::Settings;
use crate::core::examples::{self, Evidence, Outcome, Tested};
use crate::core::report::Report;
use crate::core::reqfile::Check;
use crate::core::resolve::{self, Assets, Effective};

/// The report and exit code of `reqfile test`, or the errors that prevent it.
pub fn run(cwd: &Path, only: Option<&[String]>) -> Result<(String, i32), Vec<String>> {
    let workspace = Workspace::load(cwd)?;
    if let Some(ids) = only {
        let unknown: Vec<String> = ids
            .iter()
            .filter(|id| !workspace.blocks.iter().any(|b| &b.id == *id))
            .map(|id| format!("--only: no requirement has the id {id}"))
            .collect();
        if !unknown.is_empty() {
            return Err(unknown);
        }
    }
    let blocks: Vec<&Effective> = workspace
        .blocks
        .iter()
        .filter(|b| only.is_none_or(|ids| ids.contains(&b.id)))
        .collect();
    let mut tested = Vec::new();
    for block in &blocks {
        let mut cases = Vec::new();
        for examples in example_dirs(&workspace, block) {
            if !examples.is_dir() {
                continue;
            }
            for (name, case) in sorted_dirs(&examples)? {
                let Some(label) = examples::label(&name) else {
                    return Err(vec![format!(
                        "{}: an example folder must be named violation-… or ok-…",
                        case.display()
                    )]);
                };
                let outcome = run_case(&workspace, block, &case)
                    .map(|report| examples::outcome(label, &block.id, &report))
                    .unwrap_or_else(Outcome::Error);
                cases.push((name, label, outcome));
            }
        }
        let evidence = if cases.is_empty() {
            Evidence::None
        } else if block.checks.iter().any(|c| matches!(c, Check::Decision(_))) {
            Evidence::Measured
        } else {
            Evidence::Asserted
        };
        let shared = blocks.iter().filter(|b| b.id == block.id).count() > 1;
        tested.push(Tested {
            label: if shared {
                format!("{} ({})", block.id, block.reqfile)
            } else {
                block.id.clone()
            },
            evidence,
            cases,
        });
    }
    Ok(examples::render(&tested))
}

/// The folders holding a block's labeled examples: its definition's, and
/// for a use block, its own as well.
fn example_dirs(workspace: &Workspace, block: &Effective) -> Vec<PathBuf> {
    let own = workspace
        .root
        .join(resolve::assets_dir(&block.dir, &block.id))
        .join("examples");
    let Some(imported) = &block.imported else {
        return vec![own];
    };
    let definition = match &imported.definition_assets {
        Assets::Repo(dir) => workspace.root.join(dir),
        Assets::External(dir) => dir.clone(),
    }
    .join("examples");
    vec![definition, own]
}

/// Runs a block's checks on one case: a fresh repository holding only the
/// case's files, checked with the block's effective requirement, its
/// `$REQFILE_ASSETS` and the Jev settings of its folder.
fn run_case(workspace: &Workspace, block: &Effective, case: &Path) -> Result<Report, String> {
    let repo = TempRepo::new()?;
    copy_tree(case, repo.path())?;
    let mut in_case = block.clone();
    in_case.dir = String::new();
    in_case.assets = Assets::External(workspace.assets_path(block));
    let settings = Settings::with_decision(&workspace.settings.decision(&block.dir));
    let root = repo
        .path()
        .canonicalize()
        .map_err(|e| format!("cannot resolve {}: {e}", repo.path().display()))?;
    let case_workspace = Workspace::for_example(root, settings, in_case)?;
    let options = check::Options {
        changed: None,
        only: None,
        fast: false,
        log: None,
        log_tags: Vec::new(),
    };
    Ok(check::run_in(&case_workspace, &options))
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

/// Copies a folder.
fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("cannot copy {}: {e}", from.display());
    fs::create_dir_all(to).map_err(fail)?;
    for entry in fs::read_dir(from).map_err(fail)? {
        let entry = entry.map_err(fail)?;
        let target = to.join(entry.file_name());
        let kind = entry.file_type().map_err(fail)?;
        if kind.is_symlink() {
            return Err(format!(
                "{} is a symlink; examples hold regular files only",
                entry.path().display()
            ));
        } else if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
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
