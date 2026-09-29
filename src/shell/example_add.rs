//! `reqfile example add`: turns files of the repository into a labeled
//! example of a requirement, so a miss or a false alarm met in real use
//! becomes evidence its checks are measured against from then on.

use std::fs;
use std::path::{Path, PathBuf};

use super::git;
use super::workspace::Workspace;
use crate::core::examples::{EXAMPLE_FILE, FILES_DIR};
use crate::core::paths;
use crate::core::resolve;

pub struct Request {
    pub id: String,
    pub name: String,
    pub violation: bool,
    /// Files the checks should flag, relative to the current folder.
    pub findings: Vec<PathBuf>,
    pub rationale: Option<String>,
    /// Kept out of tuning: `split: holdout`.
    pub holdout: bool,
    /// Files and folders to copy, relative to the current folder.
    pub files: Vec<PathBuf>,
}

/// Writes `.reqfile/<ID>/examples/<name>/` next to the block of the
/// requirement that applies to the first file, and says what to run next.
pub fn run(cwd: &Path, request: &Request) -> Result<String, Vec<String>> {
    let workspace = Workspace::load(cwd)?;
    let one = |e: String| vec![e];
    if request.name.is_empty()
        || request.name.contains(['/', '\\'])
        || request.name.starts_with('.')
    {
        return Err(one(format!(
            "`{}` is not a folder name for an example",
            request.name
        )));
    }
    if !request.violation && !request.findings.is_empty() {
        return Err(one(
            "--finding names where a violation is flagged; an `ok` example has none".into(),
        ));
    }
    let files: Vec<String> = request
        .files
        .iter()
        .map(|f| workspace.repository_path(cwd, f))
        .collect::<Result<_, _>>()
        .map_err(one)?;
    let first = files.first().expect("clap requires at least one file");
    let block = resolve::nearest(&workspace.blocks, paths::parent(first), &request.id)
        .map(|i| &workspace.blocks[i])
        .ok_or_else(|| one(format!("no requirement {} applies to {first}", request.id)))?;
    let in_block = |path: &str| {
        paths::relative_to(&block.dir, path)
            .map(str::to_string)
            .ok_or_else(|| {
                format!(
                    "{path} is outside {}, where {} applies",
                    paths::display_dir(&block.dir),
                    block.id
                )
            })
    };
    let relative: Vec<String> = files
        .iter()
        .map(|f| in_block(f))
        .collect::<Result<_, _>>()
        .map_err(one)?;
    let findings: Vec<String> = request
        .findings
        .iter()
        .map(|f| workspace.repository_path(cwd, f).and_then(|f| in_block(&f)))
        .collect::<Result<_, _>>()
        .map_err(one)?;
    for finding in &findings {
        if !relative
            .iter()
            .any(|r| r == finding || paths::relative_to(r, finding).is_some())
        {
            return Err(one(format!(
                "expected finding {finding} is not among the files of the example"
            )));
        }
    }
    let dir = paths::join(
        &resolve::assets_dir(&block.dir, &block.id),
        &format!("examples/{}", request.name),
    );
    let target = workspace.root.join(&dir);
    if target.exists() {
        return Err(one(format!("{dir} already exists")));
    }
    for (file, rel) in files.iter().zip(&relative) {
        copy(
            &workspace.root.join(file),
            &target.join(FILES_DIR).join(rel),
        )
        .map_err(one)?;
    }
    let mut yaml = format!(
        "expected: {}\n",
        if request.violation { "violation" } else { "ok" }
    );
    if !findings.is_empty() {
        yaml += "findings:\n";
        for finding in &findings {
            yaml += &format!("  - {}\n", quoted(finding));
        }
    }
    if let Some(rationale) = &request.rationale {
        yaml += &format!("rationale: {}\n", quoted(rationale));
    }
    if let Some(head) = git::head(&workspace.root) {
        yaml += &format!("origin: {}\n", quoted(&format!("real:{head}")));
    }
    if request.holdout {
        yaml += "split: holdout\n";
    }
    let file = target.join(EXAMPLE_FILE);
    fs::write(&file, yaml).map_err(|e| one(format!("cannot write {dir}/{EXAMPLE_FILE}: {e}")))?;
    Ok(format!(
        "Added {dir}/ ({} files).\nMeasure the checks on it:\n  reqfile eval --only {}\n",
        count_files(&target.join(FILES_DIR)),
        block.id
    ))
}

/// A YAML scalar that reads back as `text` exactly: JSON strings are YAML.
fn quoted(text: &str) -> String {
    serde_json::to_string(text).expect("a string serializes")
}

/// Copies a file or a folder to `to`, creating its parents.
fn copy(from: &Path, to: &Path) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("cannot copy {}: {e}", from.display());
    let kind = fs::symlink_metadata(from).map_err(fail)?.file_type();
    if kind.is_symlink() {
        return Err(format!(
            "{} is a symlink; examples hold regular files only",
            from.display()
        ));
    }
    if kind.is_dir() {
        for entry in fs::read_dir(from).map_err(fail)? {
            let entry = entry.map_err(fail)?;
            copy(&entry.path(), &to.join(entry.file_name()))?;
        }
        return Ok(());
    }
    fs::create_dir_all(to.parent().expect("a copied file has a parent")).map_err(fail)?;
    fs::copy(from, to).map(|_| ()).map_err(fail)
}

fn count_files(dir: &Path) -> usize {
    fs::read_dir(dir).map_or(0, |entries| {
        entries
            .flatten()
            .map(|e| {
                if e.path().is_dir() {
                    count_files(&e.path())
                } else {
                    1
                }
            })
            .sum()
    })
}
