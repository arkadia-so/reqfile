//! `reqfile eval`: runs each requirement's checks on its labeled examples,
//! each case's `files/` alone in a fresh repository, the checks' own files
//! reached only through `$REQFILE_ASSETS`, as many times as asked so the
//! variation of judgments shows.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::check;
use super::workspace::Workspace;
use crate::core::config::Settings;
use crate::core::examples::{self, Case, EXAMPLE_FILE, Evidence, FILES_DIR, Outcome, Tested};
use crate::core::report::Report;
use crate::core::reqfile::Check;
use crate::core::resolve::{self, Assets, Effective};

/// The report and exit code of `reqfile eval`, each case run `runs` times,
/// or the errors that prevent it.
pub fn run(
    cwd: &Path,
    only: Option<&[String]>,
    uses: &[String],
    runs: usize,
) -> Result<(String, i32), Vec<String>> {
    let workspace = Workspace::load_trying(cwd, uses, only)?;
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
        // A command that reports probabilities is measured like a decision check.
        let mut probabilistic = false;
        for examples in example_dirs(&workspace, block) {
            if !examples.is_dir() {
                continue;
            }
            for (name, case) in sorted_dirs(&examples)? {
                let example = read_example(&workspace, &case)?;
                let mut outcomes = Vec::new();
                for _ in 0..runs {
                    outcomes.push(match run_case(&workspace, block, &case.join(FILES_DIR)) {
                        Ok(report) => {
                            probabilistic |= report.findings.iter().any(|f| {
                                f.requirement == block.id
                                    && f.probability.is_some()
                                    && f.model.is_none()
                            });
                            examples::outcome(&example, &block.id, &report)
                        }
                        Err(e) => Outcome::Error(e),
                    });
                }
                cases.push(Case {
                    name,
                    label: example.expected,
                    known: example.known,
                    holdout: example.holdout,
                    outcomes,
                });
            }
        }
        let evidence = if cases.is_empty() {
            Evidence::None
        } else if probabilistic || block.checks.iter().any(|c| matches!(c, Check::Decision(_))) {
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
    Ok(examples::render(&tested, runs))
}

/// Reads an example's `example.yaml`, checking that it has its case in
/// `files/` and that its expected findings are files of that case.
fn read_example(workspace: &Workspace, case: &Path) -> Result<examples::Example, Vec<String>> {
    let shown = |path: &Path| {
        path.strip_prefix(&workspace.root)
            .unwrap_or(path)
            .display()
            .to_string()
    };
    let file = case.join(EXAMPLE_FILE);
    if !file.is_file() {
        return Err(vec![format!(
            "{}: an example holds {EXAMPLE_FILE} (`expected: violation` or `expected: ok`) and its case in {FILES_DIR}/; the folder name no longer carries the label",
            shown(case)
        )]);
    }
    let text = fs::read_to_string(&file).map_err(|e| vec![format!("{}: {e}", shown(&file))])?;
    let example = examples::parse(&shown(&file), &text).map_err(|e| vec![e.to_string()])?;
    let files = case.join(FILES_DIR);
    if !files.is_dir() {
        return Err(vec![format!(
            "{}: an example holds its case in {FILES_DIR}/, the only part its checks see",
            shown(case)
        )]);
    }
    for finding in &example.findings {
        if !files.join(finding).is_file() {
            return Err(vec![format!(
                "{}: expected finding {finding} is not a file of {FILES_DIR}/",
                shown(&file)
            )]);
        }
    }
    Ok(example)
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
        uses: Vec::new(),
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
