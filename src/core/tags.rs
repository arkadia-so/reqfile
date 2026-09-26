//! Tags of git sources: what a remote lists for a tag, and the last known
//! resolution of each tag, one `<owner/repo> <tag> <commit>` line per tag.

/// The commit `refs/tags/<tag>` points to in `git ls-remote` output. An
/// annotated tag points to a tag object; its peeled entry is the commit.
pub fn commit_in_listing(listing: &str, tag: &str) -> Option<String> {
    let name = format!("refs/tags/{tag}");
    let lookup = |wanted: &str| {
        listing.lines().find_map(|line| {
            let (commit, reference) = line.split_once('\t')?;
            (reference == wanted).then(|| commit.to_string())
        })
    };
    lookup(&format!("{name}^{{}}")).or_else(|| lookup(&name))
}

/// The commit `tag` of `repo` last resolved to, in a cache file's text.
pub fn cached(text: &str, repo: &str, tag: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let mut parts = line.split(' ');
        match (parts.next(), parts.next(), parts.next()) {
            (Some(r), Some(t), Some(commit)) if r == repo && t == tag => Some(commit.to_string()),
            _ => None,
        }
    })
}

/// The cache file's text with `tag` of `repo` resolved to `commit`, or
/// `None` if it already says so.
pub fn remember(text: &str, repo: &str, tag: &str, commit: &str) -> Option<String> {
    let entry = format!("{repo} {tag} {commit}");
    if text.lines().any(|line| line == entry) {
        return None;
    }
    let prefix = format!("{repo} {tag} ");
    let mut lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.starts_with(&prefix))
        .collect();
    lines.push(&entry);
    lines.sort();
    Some(lines.join("\n") + "\n")
}
