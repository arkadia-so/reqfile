//! `reqfile list`: every requirement of the repository, with its type and checks.

use std::path::Path;

use super::workspace::Workspace;
use crate::core::reqfile::{Check, Kind, Reqfile, Requirement};

/// The listing to print, or the errors that prevent it.
pub fn run(cwd: &Path, kind: Option<Kind>) -> Result<String, Vec<String>> {
    Ok(render(&Workspace::load(cwd)?.reqfiles, kind))
}

/// The requirements of `reqfiles` of the given type (all without one),
/// grouped by Reqfile, then counted.
fn render(reqfiles: &[Reqfile], kind: Option<Kind>) -> String {
    let listed: Vec<_> = reqfiles
        .iter()
        .flat_map(|r| r.requirements.iter().map(move |req| (r, req)))
        .filter(|(_, req)| kind.is_none_or(|k| req.kind == k))
        .collect();
    let what = kind.map_or(String::new(), |k| format!("{} ", k.as_str()));
    if listed.is_empty() {
        return format!("No {what}requirements in this repository.\n");
    }
    let mut out = String::new();
    let mut current = "";
    for (reqfile, requirement) in &listed {
        if reqfile.path != current {
            current = &reqfile.path;
            if !out.is_empty() {
                out += "\n";
            }
            out += &format!("From {current}:\n");
        }
        out += &format!(
            "  {} ({})  checks: {}\n",
            requirement.id,
            requirement.kind.as_str(),
            checks(requirement)
        );
    }
    let count = |k: Kind| listed.iter().filter(|(_, req)| req.kind == k).count();
    out += &format!("\n{} {what}requirements", listed.len());
    if kind.is_none() {
        out += &format!(
            ": {} product, {} code",
            count(Kind::Product),
            count(Kind::Code)
        );
    }
    out += ".\n";
    out
}

/// The types of a requirement's checks, in declaration order, without repeats.
fn checks(requirement: &Requirement) -> String {
    let mut types: Vec<&str> = Vec::new();
    for check in &requirement.checks {
        let name = match check {
            Check::Command(_) => "command",
            Check::Decision(d) if d.blocking => "decision (blocking)",
            Check::Decision(_) => "decision",
        };
        if !types.contains(&name) {
            types.push(name);
        }
    }
    types.join(", ")
}
