//! The subset of SARIF 2.1 that reqfile reads: each result's rule, message
//! and first location.

use serde::Deserialize;

use super::paths;

#[derive(Debug, PartialEq)]
pub struct SarifResult {
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
}

#[derive(Deserialize)]
struct Log {
    runs: Vec<Run>,
}

#[derive(Deserialize)]
struct Run {
    #[serde(default)]
    results: Vec<ResultEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResultEntry {
    rule_id: Option<String>,
    message: Message,
    #[serde(default)]
    locations: Vec<Location>,
}

#[derive(Deserialize)]
struct Message {
    text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Location {
    physical_location: Option<PhysicalLocation>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PhysicalLocation {
    artifact_location: Option<ArtifactLocation>,
    region: Option<Region>,
}

#[derive(Deserialize)]
struct ArtifactLocation {
    uri: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Region {
    start_line: Option<usize>,
}

/// Parses a SARIF log written by a command run from repository folder
/// `cwd`, in a repository whose absolute root is `repo_root`. Locations
/// become repository paths.
pub fn parse(text: &str, cwd: &str, repo_root: &str) -> Result<Vec<SarifResult>, String> {
    let log: Log =
        serde_json::from_str(text).map_err(|e| format!("output is not valid SARIF: {e}"))?;
    Ok(log
        .runs
        .into_iter()
        .flat_map(|run| run.results)
        .map(|result| {
            let physical = result
                .locations
                .into_iter()
                .find_map(|l| l.physical_location);
            let (file, line) = match physical {
                None => (None, None),
                Some(p) => (
                    p.artifact_location
                        .map(|a| repository_path(&a.uri, cwd, repo_root)),
                    p.region.and_then(|r| r.start_line),
                ),
            };
            let message = match result.rule_id {
                Some(rule) if !result.message.text.contains(&rule) => {
                    format!("{} ({rule})", result.message.text)
                }
                _ => result.message.text,
            };
            SarifResult {
                message,
                file,
                line,
            }
        })
        .collect())
}

fn repository_path(uri: &str, cwd: &str, repo_root: &str) -> String {
    let path = percent_decode(uri.strip_prefix("file://").unwrap_or(uri));
    let root = repo_root.trim_end_matches('/');
    if let Some(inside) = path.strip_prefix(root).and_then(|p| p.strip_prefix('/')) {
        return inside.to_string();
    }
    if path.starts_with('/') {
        return path;
    }
    paths::normalize(&paths::join(cwd, &path)).unwrap_or(path)
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok());
        match (bytes[i], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
