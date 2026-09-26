//! `reqfile explain <path>`: the requirements that apply to a path.

use std::path::Path;

use super::workspace::Workspace;
use crate::core::paths;
use crate::core::plan;

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
    let applicable = plan::applicable(&workspace.reqfiles, dir);
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
    for (reqfile, requirement) in applicable {
        if reqfile.path != current {
            current = &reqfile.path;
            out += &format!("\nFrom {current}:\n");
        }
        out += &format!(
            "  {} ({})\n    must: {}\n    why: {}\n",
            requirement.id,
            requirement.kind.as_str(),
            requirement.must,
            requirement.why
        );
    }
    Ok(out)
}
