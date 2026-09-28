//! Git sources, `owner/repo@ref`: a Reqfile pins them to a commit, fetched
//! once into the user's cache and shared by every repository and worktree,
//! addressed by commit id so other runs can change cost, never results. Tags,
//! given to `reqfile add` or `--use`, are resolved on each use.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::git;
use crate::core::reqfile::GitRef;
use crate::core::tags;

/// Where `owner/repo` is fetched from: `$REQFILE_GIT_BASE/owner/repo`, by
/// default on GitHub. A mirror or a local folder can stand in for it.
const BASE_ENV: &str = "REQFILE_GIT_BASE";
const DEFAULT_BASE: &str = "https://github.com";
/// Last known resolution of each tag: `<owner/repo> <tag> <commit>` lines.
const TAGS_FILE: &str = "tags";

/// A source at the commit its ref resolved to, with its files on disk.
pub struct Fetched {
    pub commit: String,
    pub offline: bool,
    pub dir: PathBuf,
}

/// The user's cache of sources: `$REQFILE_CACHE`, else
/// `$XDG_CACHE_HOME/reqfile`, else `~/.cache/reqfile`, then `sources/`.
pub fn cache_dir() -> Result<PathBuf, String> {
    let env = |key: &str| {
        std::env::var_os(key)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let base = env("REQFILE_CACHE")
        .or_else(|| env("XDG_CACHE_HOME").map(|d| d.join("reqfile")))
        .or_else(|| env("HOME").map(|d| d.join(".cache").join("reqfile")))
        .ok_or("cannot find a cache folder: set REQFILE_CACHE or HOME")?;
    Ok(base.join("sources"))
}

/// Resolves and fetches `repo` at `reference` into the user's cache.
pub fn fetch(repo: &str, reference: &GitRef) -> Result<Fetched, String> {
    let cache = cache_dir()?;
    let url = url(repo);
    let location = format!("{repo}@{}", reference.as_str());
    let (commit, offline) = match reference {
        GitRef::Commit(commit) => (commit.clone(), false),
        GitRef::Tag(tag) => resolve_tag(&cache, &url, repo, tag)
            .map_err(|e| format!("cannot resolve {location}: {e}"))?,
    };
    let dir = cache.join("commits").join(&commit);
    if !dir.is_dir() {
        extract(&cache, &url, &commit, &dir)
            .map_err(|e| format!("cannot fetch {location}: {e}"))?;
    }
    Ok(Fetched {
        commit,
        offline,
        dir,
    })
}

/// Where `owner/repo` is fetched from.
fn url(repo: &str) -> String {
    let base = std::env::var(BASE_ENV).unwrap_or_else(|_| DEFAULT_BASE.to_string());
    format!("{}/{repo}", base.trim_end_matches('/'))
}

/// Every tag of `repo` with the commit it points to, asked to the remote.
pub fn remote_tags(repo: &str) -> Result<Vec<(String, String)>, String> {
    let url = url(repo);
    let output = Command::new("git")
        .args(["ls-remote", "--tags", &url])
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cannot list the tags of {url}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(tags::all_in_listing(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

/// The commit a tag points to, asked to the remote on every run; the last
/// cached resolution is used only when the remote cannot be reached.
fn resolve_tag(cache: &Path, url: &str, repo: &str, tag: &str) -> Result<(String, bool), String> {
    let path = cache.join(TAGS_FILE);
    let known = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let output = Command::new("git")
        .args(["ls-remote", "--tags", url, &format!("refs/tags/{tag}")])
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    if !output.status.success() {
        let reason = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return match tags::cached(&known, repo, tag) {
            Some(commit) => Ok((commit, true)),
            None => Err(format!(
                "{url} cannot be reached and the tag was never resolved here: {reason}"
            )),
        };
    }
    let commit = tags::commit_in_listing(&String::from_utf8_lossy(&output.stdout), tag)
        .ok_or_else(|| {
            format!(
                "{url} has no tag {tag}; pin requirements to a tag or a full commit id, never a branch"
            )
        })?;
    if let Some(text) = tags::remember(&known, repo, tag, &commit) {
        let fail = |e: std::io::Error| format!("cannot write {}: {e}", path.display());
        fs::create_dir_all(cache).map_err(fail)?;
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        fs::write(&temporary, text).map_err(fail)?;
        fs::rename(&temporary, &path).map_err(fail)?;
    }
    Ok((commit, false))
}

/// Fetches a commit into the shared object store and writes its files to
/// `dir`, atomically, so concurrent runs never see half of it.
fn extract(cache: &Path, url: &str, commit: &str, dir: &Path) -> Result<(), String> {
    let store = cache.join("objects.git");
    if !store.join("HEAD").is_file() {
        fs::create_dir_all(&store)
            .map_err(|e| format!("cannot create {}: {e}", store.display()))?;
        git::git(&store, &["init", "--quiet", "--bare"])?;
    }
    let present = git::git(&store, &["cat-file", "-e", &format!("{commit}^{{commit}}")]).is_ok();
    if !present {
        git::git(
            &store,
            &[
                "fetch",
                "--quiet",
                "--no-tags",
                "--no-write-fetch-head",
                url,
                commit,
            ],
        )
        .map_err(|e| format!("commit {commit} is not in {url}: {e}"))?;
    }
    let parent = dir.parent().expect("a commit folder has a parent");
    fs::create_dir_all(parent).map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    let staging = tempfile::Builder::new()
        .prefix(".staging-")
        .tempdir_in(parent)
        .map_err(|e| format!("cannot create a folder in {}: {e}", parent.display()))?;
    let status = Command::new("sh")
        .arg("-c")
        .arg("git --git-dir=\"$1\" archive --format=tar \"$2\" | tar -x -C \"$3\"")
        .arg("sh")
        .arg(&store)
        .arg(commit)
        .arg(staging.path())
        .stdin(Stdio::null())
        .status()
        .map_err(|e| format!("could not run git archive: {e}"))?;
    if !status.success() {
        return Err(format!("cannot extract commit {commit}"));
    }
    match fs::rename(staging.path(), dir) {
        Ok(()) => {
            // Moved into place: the path it returns no longer exists, and
            // dropping the staging folder must not remove anything.
            let _moved = staging.keep();
            Ok(())
        }
        // Another run extracted the same commit meanwhile, with the same
        // content; the staging folder is removed when dropped.
        Err(_) if dir.is_dir() => Ok(()),
        Err(e) => Err(format!("cannot place {}: {e}", dir.display())),
    }
}
