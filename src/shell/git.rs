//! The repository as git sees it.

use std::path::{Path, PathBuf};
use std::process::Command;

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

fn paths(output: Vec<u8>) -> Vec<String> {
    output
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8_lossy(p).into_owned())
        .collect()
}

pub fn root(cwd: &Path) -> Result<PathBuf, String> {
    let output = git(cwd, &["rev-parse", "--show-toplevel"])
        .map_err(|_| "not inside a git repository".to_string())?;
    let root = PathBuf::from(String::from_utf8_lossy(&output).trim());
    root.canonicalize()
        .map_err(|e| format!("cannot resolve the repository root {}: {e}", root.display()))
}

/// Tracked and untracked files, never git-ignored ones.
pub fn files(root: &Path) -> Result<Vec<String>, String> {
    git(
        root,
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    )
    .map(paths)
}

/// Paths changed since the merge-base of `base` and HEAD: committed,
/// staged, unstaged, untracked and deleted.
pub fn changed(root: &Path, base: &str) -> Result<Vec<String>, String> {
    let merge_base = git(root, &["merge-base", base, "HEAD"])
        .map_err(|e| format!("cannot find the merge-base of {base} and HEAD: {e}"))?;
    let merge_base = String::from_utf8_lossy(&merge_base).trim().to_string();
    let mut changed = paths(git(
        root,
        &["diff", "-z", "--name-only", "--no-renames", &merge_base],
    )?);
    changed.extend(paths(git(
        root,
        &["ls-files", "-z", "--others", "--exclude-standard"],
    )?));
    Ok(changed)
}

/// The remote's default branch (origin/HEAD, such as `origin/main`), or
/// `None` when the remote has none; any other git failure is an error.
pub fn default_base(root: &Path) -> Result<Option<String>, String> {
    let output = Command::new("git")
        .args([
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ])
        .current_dir(root)
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    match output.status.code() {
        Some(0) => Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        )),
        // `--quiet` exits 1, silently, only when origin/HEAD is not set.
        Some(1) => Ok(None),
        _ => Err(format!(
            "cannot read origin/HEAD: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )),
    }
}

/// The folder git keeps for the repository, shared by all its worktrees.
pub fn common_dir(root: &Path) -> Result<PathBuf, String> {
    let output = git(root, &["rev-parse", "--git-common-dir"])?;
    Ok(root.join(String::from_utf8_lossy(&output).trim()))
}

/// The commit checked out, if there is one yet.
pub fn head(root: &Path) -> Option<String> {
    // A repository without commits has no HEAD; that is not an error.
    let output = git(root, &["rev-parse", "--verify", "--quiet", "HEAD"]).ok()?;
    Some(String::from_utf8_lossy(&output).trim().to_string())
}
