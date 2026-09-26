//! The repository as git sees it.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub(super) fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        // Never wait for credentials: a source that needs them is unreachable.
        .env("GIT_TERMINAL_PROMPT", "0")
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

/// The merge-base of `base` and HEAD.
pub fn merge_base(root: &Path, base: &str) -> Result<String, String> {
    let merge_base = git(root, &["merge-base", base, "HEAD"])
        .map_err(|e| format!("cannot find the merge-base of {base} and HEAD: {e}"))?;
    Ok(String::from_utf8_lossy(&merge_base).trim().to_string())
}

/// Paths changed since commit `merge_base`: committed, staged, unstaged,
/// untracked and deleted.
pub fn changed(root: &Path, merge_base: &str) -> Result<Vec<String>, String> {
    let mut changed = paths(git(
        root,
        &["diff", "-z", "--name-only", "--no-renames", merge_base],
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

/// The regular files of a commit, never symlinks or submodules.
pub fn commit_files(root: &Path, commit: &str) -> Result<Vec<String>, String> {
    let output = git(root, &["ls-tree", "-r", "-z", "--full-tree", commit])?;
    Ok(output
        .split(|b| *b == 0)
        .filter_map(|entry| {
            let entry = String::from_utf8_lossy(entry);
            let (meta, path) = entry.split_once('\t')?;
            let mode = meta.split(' ').next()?;
            matches!(mode, "100644" | "100755").then(|| path.to_string())
        })
        .collect())
}

/// The content of `paths` at `commit`, read in one git process.
pub fn read_at(
    root: &Path,
    commit: &str,
    paths: &[&str],
) -> Result<HashMap<String, Vec<u8>>, String> {
    if paths.is_empty() {
        return Ok(HashMap::new());
    }
    let mut child = Command::new("git")
        .args(["cat-file", "--batch"])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run git: {e}"))?;
    let input: String = paths.iter().map(|p| format!("{commit}:{p}\n")).collect();
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let output = child
        .wait_with_output()
        .map_err(|e| format!("could not run git: {e}"))?;
    writer
        .join()
        .expect("the writer does not panic")
        .map_err(|e| format!("could not write to git: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git cat-file failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    parse_batch(&output.stdout, paths)
}

/// The contents in `git cat-file --batch` output, one entry per path asked,
/// in order; missing paths are left out.
fn parse_batch(output: &[u8], paths: &[&str]) -> Result<HashMap<String, Vec<u8>>, String> {
    let mut contents = HashMap::new();
    let mut rest = output;
    for path in paths {
        let end = rest
            .iter()
            .position(|b| *b == b'\n')
            .ok_or("git cat-file output ended early")?;
        let header = String::from_utf8_lossy(&rest[..end]).into_owned();
        rest = &rest[end + 1..];
        if header.ends_with(" missing") {
            continue;
        }
        let size: usize = header
            .rsplit(' ')
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| format!("unexpected git cat-file header: {header}"))?;
        if rest.len() < size + 1 {
            return Err("git cat-file output ended early".into());
        }
        contents.insert(path.to_string(), rest[..size].to_vec());
        rest = &rest[size + 1..];
    }
    Ok(contents)
}
