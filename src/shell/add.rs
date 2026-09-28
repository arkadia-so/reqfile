//! `reqfile add <location> [IDS]`: writes the use blocks that take
//! requirements from a folder or repository into the Reqfile of the current
//! folder, a repository pinned to the commit its tag points to, and says
//! what their checks will run. It runs none of them.

use std::fs;
use std::path::Path;

use super::workspace::{self, Workspace};
use crate::core::paths;
use crate::core::pins;
use crate::core::reqfile::{self, Block, Check, GitRef, Kind, Location, Origin, Reqfile};

/// Writes the use blocks, or with `dry_run` only shows them; returns the
/// text to print, or the errors that prevent it.
pub fn run(
    cwd: &Path,
    location: &str,
    ids: &[String],
    dry_run: bool,
) -> Result<String, Vec<String>> {
    let workspace = Workspace::load(cwd)?;
    let here = workspace
        .repository_path(cwd, Path::new("."))
        .map_err(|e| vec![e])?;
    let parsed = reqfile::parse_location(location).map_err(|e| vec![e])?;
    // What the Reqfile says: a folder as given, a repository at its commit
    // with the tag, if any, in a comment.
    let (definitions, resolved, pinned, comment) = match &parsed {
        Location::Local(path) => {
            let folder = paths::normalize(&paths::join(&here, path))
                .ok_or_else(|| vec![format!("{path} is outside the repository")])?;
            let found = definitions_under(&workspace.reqfiles, &folder);
            (
                found,
                format!("{location} is {}", paths::display_dir(&folder)),
                location.to_string(),
                String::new(),
            )
        }
        Location::Git { repo, reference } => {
            let source = workspace::load_source(repo, reference)?;
            let found = definitions_under(&source.reqfiles, "");
            let resolved = format!("{location} resolved to commit {}", source.commit);
            let comment = match reference {
                GitRef::Tag(tag) => format!("  # {tag}"),
                GitRef::Commit(_) => String::new(),
            };
            (
                found,
                resolved,
                format!("{repo}@{}", source.commit),
                comment,
            )
        }
    };
    let chosen = choose(location, definitions, ids)?;
    let target = paths::join(&here, reqfile::FILE_NAME);
    if chosen.is_empty() {
        return Ok(format!(
            "{resolved}; it defines no code or process requirement.\n"
        ));
    }
    let present: Vec<&String> = workspace
        .reqfiles
        .iter()
        .filter(|r| r.path == target)
        .flat_map(|r| &r.blocks)
        .map(|b| &b.id)
        .collect();
    let taken: Vec<String> = chosen
        .iter()
        .filter(|(id, _, _)| present.contains(&id))
        .map(|(id, _, _)| format!("{target} already has {id}; edit or remove it there"))
        .collect();
    if !taken.is_empty() {
        return Err(taken);
    }
    let blocks: Vec<(Kind, Vec<String>)> = Kind::ALL
        .iter()
        .map(|&kind| {
            let lines = chosen
                .iter()
                .filter(|(_, k, _)| *k == kind)
                .map(|(id, _, _)| format!("  - {{ id: {id}, use: {pinned} }}{comment}"))
                .collect();
            (kind, lines)
        })
        .collect();
    if dry_run {
        return Ok(render(&resolved, &target, &blocks, &chosen, false));
    }
    let path = workspace.root.join(&target);
    let before = match fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(vec![format!("{target}: cannot read: {e}")]),
    };
    let after = pins::insert(before.as_deref(), &blocks);
    // Never leave a Reqfile the next run cannot read.
    reqfile::parse(&target, &after).map_err(|e| {
        vec![format!(
            "{target} could not take the use blocks ({e}); add them by hand:\n{}",
            render(&resolved, &target, &blocks, &chosen, false)
        )]
    })?;
    fs::write(&path, after).map_err(|e| vec![format!("{target}: cannot write: {e}")])?;
    Ok(render(&resolved, &target, &blocks, &chosen, true))
}

/// A definition that can be taken with `use`: id, type, and what each check runs.
type Importable = (String, Kind, Vec<String>);

/// The definitions to take: those listed in `ids`, or every importable one.
fn choose(
    location: &str,
    definitions: Vec<Importable>,
    ids: &[String],
) -> Result<Vec<Importable>, Vec<String>> {
    let mut chosen = Vec::new();
    let mut errors = Vec::new();
    for id in ids {
        if !definitions.iter().any(|(block_id, _, _)| block_id == id) {
            errors.push(format!("{location} defines no requirement {id}"));
        }
    }
    for (id, kind, checks) in definitions {
        if !ids.is_empty() && !ids.contains(&id) {
            continue;
        }
        if !kind.importable() {
            if ids.contains(&id) {
                errors.push(format!(
                    "{id} is a product requirement, which describes its own repository's product; only code and process requirements can be taken with `use`"
                ));
            }
            continue;
        }
        chosen.push((id, kind, checks));
    }
    if errors.is_empty() {
        Ok(chosen)
    } else {
        Err(errors)
    }
}

/// The use blocks written to (or to add to) `reqfile`, what their checks
/// run, and how to verify them.
fn render(
    resolved: &str,
    reqfile: &str,
    blocks: &[(Kind, Vec<String>)],
    chosen: &[Importable],
    written: bool,
) -> String {
    let verb = if written { "Added to" } else { "Add to" };
    let mut out = format!("{resolved}.\n\n{verb} {reqfile}:\n");
    for (kind, lines) in blocks.iter().filter(|(_, lines)| !lines.is_empty()) {
        out += &format!("\n{}:\n", kind.as_str());
        for line in lines {
            out += &format!("{line}\n");
        }
    }
    out += "\nTheir checks will run, in the folder of that Reqfile:\n";
    for (id, _, checks) in chosen {
        for check in checks {
            out += &format!("  {id}  {check}\n");
        }
    }
    let list: Vec<&str> = chosen.iter().map(|(id, _, _)| id.as_str()).collect();
    let list = list.join(",");
    out += &format!(
        "\nThen measure them on their examples and check this code:\n  reqfile eval --only {list}\n  reqfile check --only {list}\n"
    );
    out
}

/// The definitions of the Reqfiles at or under `folder`: id, type, and what
/// each check runs.
fn definitions_under(reqfiles: &[Reqfile], folder: &str) -> Vec<Importable> {
    reqfiles
        .iter()
        .filter(|r| paths::contains_dir(folder, &r.dir))
        .flat_map(|r| &r.blocks)
        .filter(|b| matches!(b.origin, Origin::Definition { .. }))
        .map(|b| (b.id.clone(), b.kind, describe_checks(b)))
        .collect()
}

fn describe_checks(block: &Block) -> Vec<String> {
    block
        .checks
        .iter()
        .flat_map(|checks| &checks.list)
        .map(|check| match check {
            Check::Command(command) => format!("command: {}", command.run),
            Check::Decision(decision) => format!(
                "decision{}: asks Jev about each code unit its decision.yaml selects, sending that code to the configured endpoint",
                if decision.blocking { " (blocking)" } else { "" }
            ),
        })
        .collect()
}
