//! The subset of SARIF 2.1 that reqfile reads: each result's rule, message
//! and first location, with result kinds and suppressions respected.

use serde::Deserialize;

use super::paths;

#[derive(Debug, PartialEq)]
pub struct SarifResult {
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    /// The probability of violation the tool gave, if it gave one.
    pub probability: Option<f64>,
}

pub struct Parsed {
    /// Distinguishes an empty report from one whose results were all ignored.
    pub had_results: bool,
    pub violations: Vec<SarifResult>,
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
    #[serde(default)]
    kind: ResultKind,
    #[serde(default)]
    suppressions: Vec<Suppression>,
    rule_id: Option<String>,
    message: Message,
    #[serde(default)]
    locations: Vec<Location>,
    #[serde(default)]
    properties: Properties,
}

#[derive(Default, Deserialize)]
struct Properties {
    probability: Option<f64>,
}

#[derive(Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
enum ResultKind {
    #[default]
    Fail,
    Pass,
    Open,
    Informational,
    NotApplicable,
    Review,
}

#[derive(Deserialize)]
struct Suppression {
    // Both locations suppress alike, but a missing or unknown kind is invalid.
    #[serde(rename = "kind")]
    _kind: SuppressionKind,
    #[serde(default)]
    status: SuppressionStatus,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum SuppressionKind {
    InSource,
    External,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
enum SuppressionStatus {
    #[default]
    Accepted,
    UnderReview,
    Rejected,
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
pub fn parse(text: &str, cwd: &str, repo_root: &str) -> Result<Parsed, String> {
    let log: Log =
        serde_json::from_str(text).map_err(|e| format!("output is not valid SARIF: {e}"))?;
    let had_results = log.runs.iter().any(|run| !run.results.is_empty());
    let results: Vec<ResultEntry> = log.runs.into_iter().flat_map(|run| run.results).collect();
    if let Some(p) = results
        .iter()
        .filter_map(|r| r.properties.probability)
        .find(|p| !(0.0..=1.0).contains(p))
    {
        return Err(format!(
            "a SARIF result has probability {p}, outside 0 to 1"
        ));
    }
    let violations = results
        .into_iter()
        // A result with a probability is judged by it, whatever its kind.
        .filter(|result| {
            (result.kind == ResultKind::Fail || result.properties.probability.is_some())
                && !result
                    .suppressions
                    .iter()
                    .any(|s| matches!(s.status, SuppressionStatus::Accepted))
        })
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
                probability: result.properties.probability,
            }
        })
        .collect();
    Ok(Parsed {
        had_results,
        violations,
    })
}

pub fn repository_path(uri: &str, cwd: &str, repo_root: &str) -> String {
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
