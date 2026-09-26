//! `reqfile add <location> [IDS]`: shows the use blocks that take
//! requirements from a folder or repository, and what their checks will
//! run. It runs nothing and writes nothing.

use std::path::Path;

use super::workspace::{self, Workspace};
use crate::core::paths;
use crate::core::reqfile::{self, Block, Check, Kind, Location, Origin, Reqfile};

/// The text to print, or the errors that prevent it.
pub fn run(cwd: &Path, location: &str, ids: &[String]) -> Result<String, Vec<String>> {
    let workspace = Workspace::load(cwd)?;
    let here = workspace
        .repository_path(cwd, Path::new("."))
        .map_err(|e| vec![e])?;
    let parsed = reqfile::parse_location(location).map_err(|e| vec![e])?;
    let (definitions, resolved) = match &parsed {
        Location::Local(path) => {
            let folder = paths::normalize(&paths::join(&here, path))
                .ok_or_else(|| vec![format!("{path} is outside the repository")])?;
            let found = definitions_under(&workspace.reqfiles, &folder);
            (
                found,
                format!("{location} is {}", paths::display_dir(&folder)),
            )
        }
        Location::Git { repo, reference } => {
            let source = workspace::load_source(&workspace.root, repo, reference)?;
            let found = definitions_under(&source.reqfiles, "");
            let resolved = format!("{location} resolved to commit {}", source.commit);
            (found, resolved)
        }
    };
    let chosen = choose(location, definitions, ids)?;
    Ok(render(
        location,
        &resolved,
        &paths::join(&here, reqfile::FILE_NAME),
        &chosen,
    ))
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

/// The use blocks to add to `reqfile`, under the matching sections, what
/// their checks run, and how to verify them.
fn render(location: &str, resolved: &str, reqfile: &str, chosen: &[Importable]) -> String {
    if chosen.is_empty() {
        return format!("{resolved}; it defines no code or process requirement.\n");
    }
    let mut out = format!("{resolved}.\n\nAdd to {reqfile}:\n");
    for kind in Kind::ALL {
        let blocks: Vec<&Importable> = chosen.iter().filter(|(_, k, _)| *k == kind).collect();
        if blocks.is_empty() {
            continue;
        }
        out += &format!("\n{}:\n", kind.as_str());
        for (id, _, _) in blocks {
            out += &format!("  - {{ id: {id}, use: {location} }}\n");
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
        "\nThen verify them on their examples and on this code:\n  reqfile test --only {list}\n  reqfile check --only {list}\n"
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
