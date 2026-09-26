//! Repository paths: relative to the repository root, `/`-separated, with
//! the root folder written as the empty string.

pub fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub fn parent(path: &str) -> &str {
    path.rfind('/').map_or("", |i| &path[..i])
}

/// `path` relative to folder `dir`, if it lies under it.
pub fn relative_to<'p>(dir: &str, path: &'p str) -> Option<&'p str> {
    if dir.is_empty() {
        return Some(path);
    }
    path.strip_prefix(dir)?.strip_prefix('/')
}

/// Whether folder `ancestor` is `dir` or one of its ancestors.
pub fn contains_dir(ancestor: &str, dir: &str) -> bool {
    ancestor.is_empty() || dir == ancestor || relative_to(ancestor, dir).is_some()
}

pub fn join(dir: &str, relative: &str) -> String {
    if dir.is_empty() {
        relative.to_string()
    } else {
        format!("{dir}/{relative}")
    }
}

/// Resolves `.` and `..` components; `None` if the path climbs above its start.
pub fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

/// A folder as shown to people: the root has no path of its own.
pub fn display_dir(dir: &str) -> &str {
    if dir.is_empty() {
        "the repository root"
    } else {
        dir
    }
}
