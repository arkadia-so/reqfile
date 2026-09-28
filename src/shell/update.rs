//! `reqfile update`: moves every use block pinned to a repository's commit
//! to the commit of that repository's latest release tag, the tag named in
//! a comment. The diff is the review: what changed is visible line by line.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use super::sources;
use super::workspace::Workspace;
use crate::core::pins;
use crate::core::tags;

/// Rewrites the pins, or with `dry_run` only says what would move; returns
/// the text to print, or the errors that prevent it.
pub fn run(cwd: &Path, dry_run: bool) -> Result<String, Vec<String>> {
    let workspace = Workspace::load(cwd)?;
    let mut releases: BTreeMap<String, Option<(String, String)>> = BTreeMap::new();
    let mut moved = Vec::new();
    let mut current = Vec::new();
    let mut errors = Vec::new();
    for reqfile in &workspace.reqfiles {
        let path = workspace.root.join(&reqfile.path);
        let mut text = fs::read_to_string(&path)
            .map_err(|e| vec![format!("{}: cannot read: {e}", reqfile.path)])?;
        let mut changed = false;
        for pin in pins::pins(&text) {
            if !releases.contains_key(&pin.repo) {
                match sources::remote_tags(&pin.repo) {
                    Ok(tags) => {
                        releases.insert(pin.repo.clone(), tags::latest(&tags).cloned());
                    }
                    Err(e) => {
                        errors.push(e);
                        releases.insert(pin.repo.clone(), None);
                        continue;
                    }
                }
            }
            let at = format!("{}:{}", reqfile.path, pin.line);
            let Some(Some((tag, commit))) = releases.get(&pin.repo) else {
                current.push(format!(
                    "  {at}  {}  no release tag; left as pinned",
                    pin.repo
                ));
                continue;
            };
            let was = pin
                .tag
                .clone()
                .unwrap_or_else(|| pin.commit[..12].to_string());
            if *commit == pin.commit {
                current.push(format!("  {at}  {}  {was}, the latest release", pin.repo));
                continue;
            }
            moved.push(format!("  {at}  {}  {was} -> {tag}", pin.repo));
            text = pins::repin(&text, pin.line, commit, tag);
            changed = true;
        }
        if changed && !dry_run {
            fs::write(&path, &text)
                .map_err(|e| vec![format!("{}: cannot write: {e}", reqfile.path)])?;
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let mut out = String::new();
    if moved.is_empty() {
        out += "Every pinned repository is at its latest release.\n";
    } else {
        out += if dry_run { "Would move:\n" } else { "Moved:\n" };
        out += &(moved.join("\n") + "\n");
        out +=
            "\nReview the diff, then measure and check again:\n  reqfile eval\n  reqfile check\n";
    }
    if !current.is_empty() {
        out += &format!("\nUp to date:\n{}\n", current.join("\n"));
    }
    Ok(out)
}
