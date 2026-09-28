//! Use blocks as Reqfile text: inserting new ones under their section, and
//! finding and moving the commit a git source is pinned to, with its tag
//! kept in a comment. Edits touch only those lines, so comments, order and
//! formatting of everything else stay as written.

use super::reqfile::Kind;

/// `text` (or a new Reqfile) with `lines` appended to the section of each
/// kind, creating sections that are missing.
pub fn insert(text: Option<&str>, blocks: &[(Kind, Vec<String>)]) -> String {
    let mut lines: Vec<String> = match text {
        Some(text) => text.lines().map(str::to_string).collect(),
        None => vec!["reqfile: 1".to_string()],
    };
    for (kind, new) in blocks {
        if new.is_empty() {
            continue;
        }
        let header = format!("{}:", kind.as_str());
        match lines.iter().position(|l| l.trim_end() == header) {
            Some(start) => {
                // The section ends at the next top-level key.
                let mut end = lines[start + 1..]
                    .iter()
                    .position(|l| top_level(l))
                    .map_or(lines.len(), |i| start + 1 + i);
                // New blocks go after the section's last line, not after the
                // blank lines and comments leading into the next section.
                while end > start + 1 && is_blank_or_comment(&lines[end - 1]) {
                    end -= 1;
                }
                for (offset, line) in new.iter().enumerate() {
                    lines.insert(end + offset, line.clone());
                }
            }
            None => {
                if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                    lines.push(String::new());
                }
                lines.push(header);
                lines.extend(new.iter().cloned());
            }
        }
    }
    lines.join("\n") + "\n"
}

fn top_level(line: &str) -> bool {
    !line.is_empty() && !line.starts_with([' ', '\t', '#', '-'])
}

fn is_blank_or_comment(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.is_empty() || trimmed.starts_with('#')
}

/// A use block line pinning a git source: `use: owner/repo@<commit>`,
/// optionally followed by `# <tag>`.
#[derive(Debug, PartialEq)]
pub struct Pin {
    /// 1-based line number.
    pub line: usize,
    pub repo: String,
    pub commit: String,
    pub tag: Option<String>,
}

/// Every git pin in a Reqfile's text.
pub fn pins(text: &str) -> Vec<Pin> {
    text.lines()
        .enumerate()
        .filter_map(|(i, line)| {
            pin_in(line).map(|(repo, commit, tag)| Pin {
                line: i + 1,
                repo,
                commit,
                tag,
            })
        })
        .collect()
}

fn pin_in(line: &str) -> Option<(String, String, Option<String>)> {
    let (code, comment) = match line.split_once('#') {
        Some((code, comment)) => (code, Some(comment.trim())),
        None => (line, None),
    };
    let rest = &code[code.find("use:")? + "use:".len()..];
    let location: String = rest
        .trim_start()
        .chars()
        .take_while(|c| !c.is_whitespace() && !matches!(c, ',' | '}'))
        .collect();
    let (repo, commit) = location.split_once('@')?;
    let is_commit = commit.len() == 40 && commit.chars().all(|c| c.is_ascii_hexdigit());
    if !is_commit || !repo.contains('/') {
        return None;
    }
    let tag = comment
        .and_then(|c| c.split_whitespace().next())
        .map(str::to_string);
    Some((repo.to_string(), commit.to_ascii_lowercase(), tag))
}

/// `text` with the pin on `line` moved to `commit`, its comment naming `tag`.
pub fn repin(text: &str, line: usize, commit: &str, tag: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let old = &lines[line - 1];
    let code = old
        .split_once('#')
        .map_or(old.as_str(), |(code, _)| code)
        .trim_end();
    let (repo, old_commit, _) = pin_in(old).expect("repin is given a pin's line");
    let code = code.replacen(
        &format!("{repo}@{old_commit}"),
        &format!("{repo}@{commit}"),
        1,
    );
    lines[line - 1] = format!("{code}  # {tag}");
    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "c182f3c70b1ffb7d86e6ed96f3d096d758f84e62";
    const NEW: &str = "d0ba113c4ca2882d8c86329049359e02ae9625b6";

    #[test]
    fn inserted_blocks_go_at_the_end_of_their_section_before_what_follows() {
        let text =
            "reqfile: 1\n\ncode:\n  - id: A\n    must: x\n\n# Process\nprocess:\n  - id: B\n";
        let added = insert(
            Some(text),
            &[(Kind::Code, vec!["  - { id: C, use: ./std }".into()])],
        );
        assert_eq!(
            added,
            "reqfile: 1\n\ncode:\n  - id: A\n    must: x\n  - { id: C, use: ./std }\n\n# Process\nprocess:\n  - id: B\n"
        );
    }

    #[test]
    fn a_missing_section_or_reqfile_is_created() {
        let blocks = [(Kind::Process, vec!["  - { id: P, use: ./std }".into()])];
        assert_eq!(
            insert(Some("reqfile: 1\ncode:\n  - id: A\n"), &blocks),
            "reqfile: 1\ncode:\n  - id: A\n\nprocess:\n  - { id: P, use: ./std }\n"
        );
        assert_eq!(
            insert(None, &blocks),
            "reqfile: 1\n\nprocess:\n  - { id: P, use: ./std }\n"
        );
    }

    #[test]
    fn pins_are_found_in_flow_and_block_style_with_their_tag() {
        let text = format!(
            "code:\n  - {{ id: A, use: acme/reqs@{SHA} }}  # v1.2.0\n  - id: B\n    use: acme/other@{SHA}\n  - {{ id: C, use: ./std }}\n"
        );
        assert_eq!(
            pins(&text),
            [
                Pin {
                    line: 2,
                    repo: "acme/reqs".into(),
                    commit: SHA.into(),
                    tag: Some("v1.2.0".into())
                },
                Pin {
                    line: 4,
                    repo: "acme/other".into(),
                    commit: SHA.into(),
                    tag: None
                },
            ]
        );
    }

    #[test]
    fn repin_moves_the_commit_and_names_the_tag() {
        let text = format!("code:\n  - {{ id: A, use: acme/reqs@{SHA} }}  # v1\n");
        assert_eq!(
            repin(&text, 2, NEW, "v2"),
            format!("code:\n  - {{ id: A, use: acme/reqs@{NEW} }}  # v2\n")
        );
    }
}
