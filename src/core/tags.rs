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

/// Every tag of a `git ls-remote --tags` listing with the commit it points
/// to, annotated tags peeled.
pub fn all_in_listing(listing: &str) -> Vec<(String, String)> {
    let mut tags: Vec<(String, String)> = Vec::new();
    for line in listing.lines() {
        let Some((commit, reference)) = line.split_once('\t') else {
            continue;
        };
        let Some(name) = reference.strip_prefix("refs/tags/") else {
            continue;
        };
        match name.strip_suffix("^{}") {
            // The peeled entry of an annotated tag names the commit.
            Some(tag) => match tags.iter_mut().find(|(t, _)| t == tag) {
                Some(entry) => entry.1 = commit.to_string(),
                None => tags.push((tag.to_string(), commit.to_string())),
            },
            None if !tags.iter().any(|(t, _)| t == name) => {
                tags.push((name.to_string(), commit.to_string()))
            }
            None => {}
        }
    }
    tags
}

/// The release a pin moves to: the highest `vX.Y.Z` or `X.Y.Z` tag, any
/// number of numeric parts, pre-releases (`-rc1`) and other names ignored.
pub fn latest(tags: &[(String, String)]) -> Option<&(String, String)> {
    let version = |tag: &str| -> Option<Vec<u64>> {
        let digits = tag.strip_prefix('v').unwrap_or(tag);
        digits.split('.').map(|part| part.parse().ok()).collect()
    };
    tags.iter()
        .filter_map(|entry| version(&entry.0).map(|v| (v, entry)))
        .max_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(_, entry)| entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listings_are_read_with_annotated_tags_peeled() {
        let listing = "aaa\trefs/tags/v1.0.0\nbbb\trefs/tags/v2.0.0\nccc\trefs/tags/v2.0.0^{}\n";
        assert_eq!(
            all_in_listing(listing),
            [
                ("v1.0.0".to_string(), "aaa".to_string()),
                ("v2.0.0".to_string(), "ccc".to_string())
            ]
        );
    }

    #[test]
    fn the_latest_release_is_the_highest_version_not_the_last_listed() {
        let tags: Vec<(String, String)> = [
            ("v0.10.0", "a"),
            ("v0.9.0", "b"),
            ("v1.0.0-rc1", "c"),
            ("nightly", "d"),
            ("0.2", "e"),
        ]
        .iter()
        .map(|(t, c)| (t.to_string(), c.to_string()))
        .collect();
        assert_eq!(latest(&tags).map(|t| t.0.as_str()), Some("v0.10.0"));
        assert_eq!(latest(&[]), None);
    }
}
