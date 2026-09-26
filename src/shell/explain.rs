//! `reqfile explain <path>`: the requirements that apply to a path, and
//! where each one and its checks come from.

use std::path::Path;

use super::workspace::Workspace;
use crate::core::paths;
use crate::core::resolve;

/// The explanation to print, or the errors that prevent it.
pub fn run(cwd: &Path, path: &Path) -> Result<String, Vec<String>> {
    let workspace = Workspace::load(cwd)?;
    let target = workspace.repository_path(cwd, path).map_err(|e| vec![e])?;
    if let Some(config) = workspace.settings.excluded_by(&target) {
        return Ok(format!(
            "{target} is excluded by {config}; no requirements apply.\n"
        ));
    }
    let dir = if workspace.root.join(&target).is_dir() {
        target.as_str()
    } else {
        paths::parent(&target)
    };
    let applicable = resolve::applicable(&workspace.blocks, dir);
    if applicable.is_empty() {
        return Ok(format!(
            "No requirements apply to {}.\n",
            paths::display_dir(&target)
        ));
    }
    let mut out = format!(
        "Requirements that apply to {}:\n",
        paths::display_dir(&target)
    );
    let mut current = "";
    for block in applicable {
        if block.reqfile != current {
            current = &block.reqfile;
            out += &format!("\nFrom {current}:\n");
        }
        out += &format!(
            "  {} ({})\n    must: {}\n    why: {}\n",
            block.id,
            block.kind.as_str(),
            block.must,
            block.why
        );
        if let Some(imported) = &block.imported {
            out += &format!("    use: {}\n", imported.describe());
            out += if imported.checks_inherited {
                "    checks: inherited from the definition\n"
            } else {
                "    checks: set in this block\n"
            };
        }
    }
    Ok(out)
}
