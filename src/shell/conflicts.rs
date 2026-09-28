//! Asks Jev whether fixing each finding would break another requirement
//! that applies to its file, and records the conflicts on the findings.

use std::fs;

use serde_json::json;

use super::ask;
use super::jev_cache::Caches;
use super::pool;
use super::workspace::Workspace;
use crate::core::config::DecisionConfig;
use crate::core::conflict::{self, Other};
use crate::core::decision::{self, Asked};
use crate::core::paths;
use crate::core::report::{Error, FindingKind, Report};
use crate::core::resolve;

/// A finding to ask about: its index in the report, the Jev settings of its
/// requirement's folder, and what to ask.
struct Pending<'w> {
    finding: usize,
    config: &'w DecisionConfig,
    questions: decision::Questions,
}

/// Looks for conflicts on every violation and advisory finding with a file
/// that other requirements also apply to. Without a Jev key or cache, says
/// why in the report instead. Each answer found or asked goes to `answered`
/// as (finding index, other requirement, probability), for the run log.
pub fn check(
    workspace: &Workspace,
    report: &mut Report,
    caches: Result<&mut Caches, &String>,
    answered: &mut Vec<(usize, String, f64)>,
) {
    let settings: Vec<DecisionConfig> = report
        .findings
        .iter()
        .map(|f| {
            let dir = f.file.as_deref().map_or("", paths::parent);
            let block = resolve::nearest(&workspace.blocks, dir, &f.requirement)
                .map(|i| &workspace.blocks[i]);
            workspace
                .settings
                .decision(block.map_or("", |b| b.dir.as_str()))
        })
        .collect();
    let mut pending: Vec<Pending> = Vec::new();
    for (index, finding) in report.findings.iter().enumerate() {
        let Some(file) = &finding.file else { continue };
        if finding.kind == FindingKind::Uncertain {
            continue;
        }
        let dir = paths::parent(file);
        let Some(own) = resolve::nearest(&workspace.blocks, dir, &finding.requirement) else {
            continue;
        };
        let others: Vec<Other> = resolve::applicable(&workspace.blocks, dir)
            .into_iter()
            .filter(|b| b.id != finding.requirement)
            .map(|b| Other {
                id: &b.id,
                must: &b.must,
                why: &b.why,
            })
            .collect();
        if others.is_empty() {
            continue;
        }
        let source = fs::read_to_string(workspace.root.join(file)).ok();
        pending.push(Pending {
            finding: index,
            config: &settings[index],
            questions: conflict::questions(
                finding,
                &workspace.blocks[own].must,
                source.as_deref(),
                &others,
            ),
        });
    }
    if pending.is_empty() {
        return;
    }
    let caches = match caches {
        Ok(caches) => caches,
        Err(e) => {
            report.conflicts_not_checked = Some(e.clone());
            return;
        }
    };
    let mut failures: Vec<String> = Vec::new();
    let mut groups: Vec<(&DecisionConfig, Vec<&Pending>)> = Vec::new();
    for item in &pending {
        match groups.iter_mut().find(|(config, _)| *config == item.config) {
            Some((_, members)) => members.push(item),
            None => groups.push((item.config, vec![item])),
        }
    }
    for (config, members) in groups {
        let client = match ask::connect(config) {
            Ok(client) => client,
            Err(_) => {
                report.conflicts_not_checked = Some(format!("{} is not set", config.api_key_env));
                continue;
            }
        };
        let model = match ask::resolve_model(&client, config) {
            Ok(model) => model,
            Err(e) => {
                failures.push(e);
                continue;
            }
        };
        let asked: Vec<Asked> = members
            .iter()
            .map(|item| {
                let keys = item
                    .questions
                    .questions
                    .iter()
                    .map(|(id, q)| decision::cache_key(&model, &item.questions.state, id, q))
                    .collect();
                Asked {
                    questions: decision::Questions {
                        state: item.questions.state.clone(),
                        questions: item.questions.questions.clone(),
                    },
                    keys,
                }
            })
            .collect();
        let mut results: Vec<Vec<Option<ask::Answer>>> = asked
            .iter()
            .map(|a| {
                a.keys
                    .iter()
                    .map(|key| {
                        caches.get(key).map(|probability| {
                            Ok(ask::Judged {
                                probability,
                                model: model.clone(),
                                cached: true,
                            })
                        })
                    })
                    .collect()
            })
            .collect();
        let waiting: Vec<usize> = (0..asked.len())
            .filter(|&i| results[i].iter().any(Option::is_none))
            .collect();
        let responses = pool::map(&waiting, config.concurrency, |&i| {
            ask::send(&client, config, &asked[i], &results[i])
        });
        for (&i, response) in waiting.iter().zip(responses) {
            ask::record(caches, &asked[i], &mut results[i], response);
        }
        for ((item, asked), results) in members.iter().zip(&asked).zip(results) {
            let mut probabilities = Vec::new();
            for ((id, _), result) in asked.questions.questions.iter().zip(results) {
                match result.expect("every question was answered or failed") {
                    Ok(judged) => {
                        answered.push((item.finding, id.clone(), judged.probability));
                        probabilities.push((id.clone(), judged.probability));
                    }
                    Err(e) => failures.push(e),
                }
            }
            report.findings[item.finding].conflicts = conflict::conflicts(probabilities);
        }
    }
    if let Some(first) = failures.first() {
        // Conflicts inform a decision; failing to look for them never blocks a run.
        report.errors.push(Error {
            requirement: None,
            message: format!(
                "{} conflict questions could not be answered, first: {first}",
                failures.len()
            ),
            blocking: false,
        });
    }
}

/// The run-log fields of one conflict answer.
pub fn log_fields(
    report: &Report,
    finding: usize,
    other: &str,
    probability: f64,
) -> serde_json::Value {
    let f = &report.findings[finding];
    json!({
        "requirement": f.requirement,
        "file": f.file,
        "line": f.line,
        "breaks": other,
        "probability": probability,
        "conflict": probability > conflict::CONFLICT_ABOVE,
    })
}
