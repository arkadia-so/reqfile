//! Conflicts between requirements: whether making the fix a finding asks
//! for would break another requirement that applies to the same file, and
//! the product decisions those conflicts call for.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::json;

use super::decision::Questions;
use super::report::Finding;

/// Above this probability, the fix would break the other requirement.
pub const CONFLICT_ABOVE: f64 = 0.7;

/// The most of the file sent with the question; the rest is cut, since the
/// question is about where the code lives and what the fix changes, not
/// about every line of it.
const MAX_SOURCE_CHARS: usize = 20_000;

/// Another requirement the fix of a finding would break.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Conflict {
    pub requirement: String,
    pub probability: f64,
}

/// A requirement that applies to the file of a finding.
pub struct Other<'a> {
    pub id: &'a str,
    pub must: &'a str,
    pub why: &'a str,
}

/// What Jev is told about a finding, and one question per other
/// requirement, keyed by its id.
pub fn questions(
    finding: &Finding,
    must: &str,
    source: Option<&str>,
    others: &[Other],
) -> Questions {
    let mut state = json!({
        "file": finding.file,
        "finding": {
            "requirement": finding.requirement,
            "must": must,
            "line": finding.line,
            "message": finding.message,
            "fix": finding.fix_hint,
        },
    });
    if let Some(source) = source {
        state["source"] = json!(cut(source));
    }
    let questions = others
        .iter()
        .map(|other| {
            let question = json!({
                "type": "noul",
                "instructions": {
                    "subject": "the change `finding.fix` asks for in `file`",
                    "question": format!(
                        "Would making this change break this other requirement? {}: {} (why: {})",
                        other.id, other.must, other.why
                    ),
                },
                "criteria": {
                    "true": "Making the change the way the finding asks would leave the file, or the code around it, violating the other requirement, so the two requirements cannot both hold here.",
                    "false": "The change can be made while still meeting the other requirement, or the other requirement does not concern this change.",
                },
            });
            (other.id.to_string(), question)
        })
        .collect();
    Questions { state, questions }
}

fn cut(source: &str) -> String {
    match source.char_indices().nth(MAX_SOURCE_CHARS) {
        None => source.to_string(),
        Some((at, _)) => format!("{}\n… (cut)", &source[..at]),
    }
}

/// The conflicts among the answers, highest probability first.
pub fn conflicts(answers: impl IntoIterator<Item = (String, f64)>) -> Vec<Conflict> {
    let mut found: Vec<Conflict> = answers
        .into_iter()
        .filter(|(_, p)| *p > CONFLICT_ABOVE)
        .map(|(requirement, probability)| Conflict {
            requirement,
            probability,
        })
        .collect();
    found.sort_by(|a, b| b.probability.total_cmp(&a.probability));
    found
}

/// Two requirements that cannot both hold on some files: a product
/// decision, since no fix satisfies both.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Decision {
    /// The requirement whose findings would need the fix.
    pub requirement: String,
    /// The requirement that fix would break.
    pub breaks: String,
    pub files: Vec<String>,
    pub probability: f64,
}

/// The conflicts of every finding, grouped by pair of requirements.
pub fn decisions(findings: &[Finding]) -> Vec<Decision> {
    let mut pairs: BTreeMap<(String, String), Decision> = BTreeMap::new();
    for finding in findings {
        for conflict in &finding.conflicts {
            let decision = pairs
                .entry((finding.requirement.clone(), conflict.requirement.clone()))
                .or_insert_with(|| Decision {
                    requirement: finding.requirement.clone(),
                    breaks: conflict.requirement.clone(),
                    files: Vec::new(),
                    probability: 0.0,
                });
            if let Some(file) = &finding.file
                && !decision.files.contains(file)
            {
                decision.files.push(file.clone());
            }
            decision.probability = decision.probability.max(conflict.probability);
        }
    }
    pairs.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::report::FindingKind;

    fn finding(requirement: &str, file: &str, conflicts: &[(&str, f64)]) -> Finding {
        Finding {
            requirement: requirement.into(),
            kind: FindingKind::Violation,
            file: Some(file.into()),
            line: Some(1),
            message: "m".into(),
            fix_hint: "f".into(),
            probability: None,
            model: None,
            block: None,
            source: None,
            conflicts: conflicts
                .iter()
                .map(|(r, p)| Conflict {
                    requirement: (*r).into(),
                    probability: *p,
                })
                .collect(),
        }
    }

    #[test]
    fn only_answers_above_the_threshold_are_conflicts() {
        let found = conflicts([("A".into(), 0.9), ("B".into(), 0.7), ("C".into(), 0.95)]);
        assert_eq!(
            found
                .iter()
                .map(|c| c.requirement.as_str())
                .collect::<Vec<_>>(),
            ["C", "A"]
        );
    }

    #[test]
    fn decisions_group_files_by_pair_and_keep_the_highest_probability() {
        let findings = [
            finding("COLOCATION", "core/a.rs", &[("CORE", 0.8)]),
            finding("COLOCATION", "core/b.rs", &[("CORE", 0.9)]),
            finding("OTHER", "x.rs", &[]),
        ];
        assert_eq!(
            decisions(&findings),
            [Decision {
                requirement: "COLOCATION".into(),
                breaks: "CORE".into(),
                files: vec!["core/a.rs".into(), "core/b.rs".into()],
                probability: 0.9,
            }]
        );
    }
}
