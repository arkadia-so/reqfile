//! `reqfile list`: every requirement of the repository, with its type,
//! checks, and source for requirements taken with `use`.

use std::path::Path;

use serde_json::json;

use super::workspace::Workspace;
use crate::core::reqfile::{Check, Kind};
use crate::core::resolve::Effective;

/// The listing to print, or the errors that prevent it.
pub fn run(cwd: &Path, kind: Option<Kind>, json: bool) -> Result<String, Vec<String>> {
    let workspace = Workspace::load(cwd)?;
    let listed: Vec<&Effective> = workspace
        .blocks
        .iter()
        .filter(|b| kind.is_none_or(|k| b.kind == k))
        .collect();
    Ok(if json {
        render_json(&listed)
    } else {
        render(&listed, kind)
    })
}

/// The blocks grouped by Reqfile, then counted.
fn render(listed: &[&Effective], kind: Option<Kind>) -> String {
    let what = kind.map_or(String::new(), |k| format!("{} ", k.as_str()));
    if listed.is_empty() {
        return format!("No {what}requirements in this repository.\n");
    }
    let mut out = String::new();
    let mut current = "";
    for block in listed {
        if block.reqfile != current {
            current = &block.reqfile;
            if !out.is_empty() {
                out += "\n";
            }
            out += &format!("From {current}:\n");
        }
        let (source, inherited) = match &block.imported {
            Some(imported) => (
                format!("  use: {}", imported.location.describe()),
                if imported.checks_inherited {
                    " (inherited)"
                } else {
                    " (set here)"
                },
            ),
            None => (String::new(), ""),
        };
        out += &format!(
            "  {} ({}){source}  checks: {}{inherited}\n",
            block.id,
            block.kind.as_str(),
            check_types(block).join(", ")
        );
    }
    let count = |k: Kind| listed.iter().filter(|b| b.kind == k).count();
    out += &format!("\n{} {what}requirements", listed.len());
    if kind.is_none() {
        out += &format!(
            ": {} product, {} code",
            count(Kind::Product),
            count(Kind::Code)
        );
        if count(Kind::Process) > 0 {
            out += &format!(", {} process", count(Kind::Process));
        }
    }
    out += ".\n";
    out
}

fn render_json(listed: &[&Effective]) -> String {
    let requirements: Vec<serde_json::Value> = listed
        .iter()
        .map(|block| {
            let imported = block.imported.as_ref().map(|i| {
                json!({
                    "use": i.location.describe(),
                    "definition": i.definition,
                    "commit": i.commit,
                    "checks_inherited": i.checks_inherited,
                })
            });
            json!({
                "reqfile": block.reqfile,
                "line": block.line,
                "id": block.id,
                "kind": block.kind.as_str(),
                "must": block.must,
                "why": block.why,
                "who": block.who,
                "ref": block.reference,
                "checks": check_types(block),
                "imported": imported,
            })
        })
        .collect();
    serde_json::to_string_pretty(&json!({ "requirements": requirements }))
        .expect("the listing serializes to JSON")
        + "\n"
}

/// The types of a block's checks, in declaration order, without repeats.
fn check_types(block: &Effective) -> Vec<&'static str> {
    let mut types: Vec<&str> = Vec::new();
    for check in &block.checks {
        let name = match check {
            Check::Command(_) => "command",
            Check::Decision(d) if d.blocking => "decision (blocking)",
            Check::Decision(_) => "decision",
        };
        if !types.contains(&name) {
            types.push(name);
        }
    }
    types
}
