//! Asking Jev about the units of decision checks: one request per unit for
//! all the checks that selected it, answers reused from the local cache.

use std::path::Path;

use super::jev;
use super::jev_cache::Caches;
use super::pool;
use crate::core::config::DecisionConfig;
use crate::core::decision::{self, Asked, Batch, DecisionSpec, Unit};

/// A probability of violation and the exact model that gave it.
#[derive(Clone)]
pub struct Judged {
    pub probability: f64,
    pub model: String,
    /// Whether the answer came from the cache rather than a fresh call.
    pub cached: bool,
}

/// A unit's judgment, or why it is unknown.
pub type Answer = Result<Judged, String>;

/// A decision check to answer.
pub struct Check<'a> {
    pub id: &'a str,
    pub spec: &'a DecisionSpec,
    /// How to call Jev, from the settings of that folder.
    pub jev: &'a DecisionConfig,
    pub units: &'a [Unit],
}

/// An answer for every unit of every check, plus errors that concern no
/// check in particular. Checks are asked in groups sharing the same Jev
/// settings, all through one cache; with `prune`, the cache keeps only the
/// answers this run used.
pub fn answers(root: &Path, checks: &[Check], prune: bool) -> (Vec<Vec<Answer>>, Vec<String>) {
    let sizes: Vec<usize> = checks.iter().map(|c| c.units.len()).collect();
    if sizes.iter().all(|&n| n == 0) {
        return (checks.iter().map(|_| Vec::new()).collect(), Vec::new());
    }
    let mut caches = match Caches::open(root) {
        Ok(caches) => caches,
        Err(e) => {
            let failed = sizes.iter().map(|&n| vec![Err(e.clone()); n]).collect();
            return (failed, Vec::new());
        }
    };
    let mut groups: Vec<(&DecisionConfig, Vec<usize>)> = Vec::new();
    for (i, check) in checks.iter().enumerate() {
        match groups.iter_mut().find(|(config, _)| *config == check.jev) {
            Some((_, members)) => members.push(i),
            None => groups.push((check.jev, vec![i])),
        }
    }
    let mut answers: Vec<Option<Vec<Answer>>> = checks.iter().map(|_| None).collect();
    for (config, members) in groups {
        let group: Vec<&Check> = members.iter().map(|&i| &checks[i]).collect();
        for (&i, check_answers) in members
            .iter()
            .zip(answer_group(&mut caches, config, &group))
        {
            answers[i] = Some(check_answers);
        }
    }
    let answers = answers
        .into_iter()
        .map(|a| a.expect("every check belongs to a group"))
        .collect();
    // Pruned once, after every group used the cache.
    (answers, caches.save(prune).into_iter().collect())
}

/// The answers of each check of a group sharing one Jev configuration.
fn answer_group(
    caches: &mut Caches,
    config: &DecisionConfig,
    checks: &[&Check],
) -> Vec<Vec<Answer>> {
    let units: Vec<&[Unit]> = checks.iter().map(|c| c.units).collect();
    let sizes: Vec<usize> = units.iter().map(|u| u.len()).collect();
    let batches = decision::batch(&units);
    let per_batch = match ask_batches(caches, config, checks, &batches) {
        Ok(per_batch) => per_batch,
        Err(e) => batches
            .iter()
            .map(|b| vec![Err(e.clone()); b.members.len()])
            .collect(),
    };
    decision::spread(&batches, &sizes, per_batch)
}

/// An answer for each member of each batch, or why Jev cannot be asked at all.
fn ask_batches(
    caches: &mut Caches,
    config: &DecisionConfig,
    checks: &[&Check],
    batches: &[Batch],
) -> Result<Vec<Vec<Answer>>, String> {
    if batches.is_empty() {
        return Ok(Vec::new());
    }
    let client = connect(config)?;
    let model = resolve_model(&client, config)?;
    let asked: Vec<Asked> = batches
        .iter()
        .map(|batch| {
            let members: Vec<(&str, &DecisionSpec)> = batch
                .members
                .iter()
                .map(|&(check, _)| (checks[check].id, checks[check].spec))
                .collect();
            decision::ask(batch.unit, &members, &model)
        })
        .collect();
    let mut results: Vec<Vec<Option<Answer>>> = batches
        .iter()
        .zip(&asked)
        .map(|(batch, asked)| from_cache(caches, batch, asked, &model))
        .collect();
    let pending: Vec<usize> = (0..batches.len())
        .filter(|&i| results[i].iter().any(Option::is_none))
        .collect();
    let responses = pool::map(&pending, config.concurrency, |&i| {
        send(&client, config, &asked[i], &results[i])
    });
    for (&i, response) in pending.iter().zip(responses) {
        record(caches, &asked[i], &mut results[i], response);
    }
    Ok(results
        .into_iter()
        .map(|r| {
            r.into_iter()
                .map(|a| a.expect("every question was answered or failed"))
                .collect()
        })
        .collect())
}

fn connect(config: &DecisionConfig) -> Result<jev::Client, String> {
    match std::env::var(&config.api_key_env) {
        Ok(key) if !key.is_empty() => Ok(jev::Client::new(&config.endpoint, key)),
        _ => Err(format!(
            "{} is not set; decision checks call Jev with it",
            config.api_key_env
        )),
    }
}

/// The exact model the configured one resolves to today, which cached
/// answers are keyed by.
fn resolve_model(client: &jev::Client, config: &DecisionConfig) -> Result<String, String> {
    let body = client.ask(&decision::probe_request(&config.model))?;
    decision::parse_response(&body, &[]).map(|(model, _)| model)
}

/// The cached answer of each member, `None` where Jev must be asked. Keys
/// include `model`, so a cached answer comes from that model.
fn from_cache(
    caches: &mut Caches,
    batch: &Batch,
    asked: &Asked,
    model: &str,
) -> Vec<Option<Answer>> {
    if let Some(reason) = decision::too_large(batch.unit) {
        return vec![Some(Err(reason)); asked.keys.len()];
    }
    asked
        .keys
        .iter()
        .map(|key| {
            caches.get(key).map(|probability| {
                Ok(Judged {
                    probability,
                    model: model.to_string(),
                    cached: true,
                })
            })
        })
        .collect()
}

/// Asks the questions no cache answered.
fn send(
    client: &jev::Client,
    config: &DecisionConfig,
    asked: &Asked,
    cached: &[Option<Answer>],
) -> Result<(String, Vec<f64>), String> {
    let unanswered: Vec<&(String, serde_json::Value)> = asked
        .questions
        .questions
        .iter()
        .zip(cached)
        .filter(|(_, answer)| answer.is_none())
        .map(|(question, _)| question)
        .collect();
    let ids: Vec<&str> = unanswered.iter().map(|(id, _)| id.as_str()).collect();
    let request = decision::request(&config.model, &asked.questions.state, &unanswered);
    client
        .ask(&request)
        .and_then(|body| decision::parse_response(&body, &ids))
}

/// Fills the unanswered slots with a response, caching what Jev answered.
fn record(
    caches: &mut Caches,
    asked: &Asked,
    results: &mut [Option<Answer>],
    response: Result<(String, Vec<f64>), String>,
) {
    let slots = results
        .iter_mut()
        .zip(&asked.questions.questions)
        .filter(|(result, _)| result.is_none());
    match response {
        Ok((answered_by, probabilities)) => {
            for ((slot, (_, question)), p) in slots.zip(probabilities) {
                // Keyed by the model that answered, in case `latest` moved during the run.
                let key = decision::cache_key(&answered_by, &asked.questions.state, question);
                caches.insert(key, p);
                *slot = Some(Ok(Judged {
                    probability: p,
                    model: answered_by.clone(),
                    cached: false,
                }));
            }
        }
        Err(e) => {
            for (slot, _) in slots {
                *slot = Some(Err(e.clone()));
            }
        }
    }
}
