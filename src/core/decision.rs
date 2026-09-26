//! Decision checks: `.reqfile/<ID>/decision.yaml`, the code units it
//! selects, the question sent to Jev about each unit, and what its answer means.

use std::collections::HashMap;
use std::ops::Range;

use ast_grep_config::{GlobalRules, RuleConfig, SerializableRuleConfig};
use ast_grep_core::tree_sitter::LanguageExt;
use ast_grep_language::{Language, SupportLang};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::error::ConfigError;
use super::reqfile::FileGlob;
use super::yaml;

/// The most code (unit and enclosing function) sent in one request, about
/// 20k tokens: Jev reads at most 32k tokens of state plus question. Larger
/// units are never cut, since a violation in the cut part would pass unseen.
const MAX_SOURCE_CHARS: usize = 60_000;

/// Node kinds that count as a function when looking for a unit's enclosing function.
const FUNCTION_KINDS: &[&str] = &[
    "function_item",
    "function_declaration",
    "function_definition",
    "function_expression",
    "generator_function_declaration",
    "method_definition",
    "method_declaration",
    "arrow_function",
];

pub fn spec_path(reqfile_dir: &str, id: &str) -> String {
    super::paths::join(reqfile_dir, &format!(".reqfile/{id}/decision.yaml"))
}

pub struct DecisionSpec {
    units: Vec<UnitRule>,
    enclosing: bool,
    question: String,
    violation_when: String,
    ok_when: String,
    pub fix_hint: String,
    violation_above: f64,
    pass_below: f64,
}

struct UnitRule {
    language: SupportLang,
    matcher: ast_grep_config::RuleCore,
    files: Option<FileGlob>,
    ignores: Option<FileGlob>,
}

pub fn parse_spec(path: &str, text: &str) -> Result<DecisionSpec, ConfigError> {
    let root = yaml::parse(path, text)?;
    let mut fields = root.fields(
        path,
        "decision.yaml",
        &[
            "units",
            "context",
            "question",
            "violation_when",
            "ok_when",
            "fix_hint",
            "thresholds",
        ],
    )?;
    let units_node = fields.required("units")?;
    let units_line = units_node.line;
    let units = units_node
        .list(path, "units")?
        .into_iter()
        .map(|node| unit_rule(path, node))
        .collect::<Result<Vec<_>, _>>()?;
    if units.is_empty() {
        return Err(ConfigError::at(
            path,
            units_line,
            "`units` must list at least one ast-grep rule",
        ));
    }
    let enclosing = match fields.optional("context") {
        None => false,
        Some(node) => {
            let line = node.line;
            match node.text(path, "context")?.as_str() {
                "enclosing" => true,
                other => {
                    return Err(ConfigError::at(
                        path,
                        line,
                        format!("unknown context `{other}`; expected `enclosing`"),
                    ));
                }
            }
        }
    };
    let question = fields.required("question")?.text(path, "question")?;
    let violation_when = fields
        .required("violation_when")?
        .text(path, "violation_when")?;
    let ok_when = fields.required("ok_when")?.text(path, "ok_when")?;
    let fix_hint = fields.required("fix_hint")?.text(path, "fix_hint")?;
    let thresholds = fields.required("thresholds")?;
    let thresholds_line = thresholds.line;
    let mut thresholds =
        thresholds.fields(path, "thresholds", &["violation_above", "pass_below"])?;
    let violation_above = thresholds
        .required("violation_above")?
        .number(path, "violation_above")?;
    let pass_below = thresholds
        .required("pass_below")?
        .number(path, "pass_below")?;
    if !(0.0..=1.0).contains(&pass_below)
        || !(0.0..=1.0).contains(&violation_above)
        || pass_below > violation_above
    {
        return Err(ConfigError::at(
            path,
            thresholds_line,
            "thresholds must satisfy 0 <= pass_below <= violation_above <= 1",
        ));
    }
    Ok(DecisionSpec {
        units,
        enclosing,
        question,
        violation_when,
        ok_when,
        fix_hint,
        violation_above,
        pass_below,
    })
}

fn unit_rule(path: &str, node: yaml::Node) -> Result<UnitRule, ConfigError> {
    let line = node.line;
    let mut fields = node.fields(
        path,
        "a unit",
        &[
            "language",
            "rule",
            "constraints",
            "utils",
            "files",
            "ignores",
        ],
    )?;
    // A glob or a list of globs, as in ast-grep rules.
    let glob = |fields: &mut yaml::Fields, key: &str| -> Result<Option<FileGlob>, ConfigError> {
        let Some(node) = fields.optional(key) else {
            return Ok(None);
        };
        let line = node.line;
        let patterns = match node.value {
            yaml::Value::Seq(items) => items
                .into_iter()
                .map(|item| item.text(path, key))
                .collect::<Result<Vec<_>, _>>()?,
            _ => vec![node.text(path, key)?],
        };
        let patterns: Vec<&str> = patterns.iter().map(String::as_str).collect();
        FileGlob::any(&patterns)
            .map(Some)
            .map_err(|e| ConfigError::at(path, line, format!("invalid `{key}` glob: {e}")))
    };
    let files = glob(&mut fields, "files")?;
    let ignores = glob(&mut fields, "ignores")?;
    let mut rule = serde_json::Map::new();
    rule.insert("id".into(), json!("unit"));
    for key in ["language", "rule", "constraints", "utils"] {
        if let Some(value) = fields.optional(key) {
            rule.insert(key.into(), value.into_json());
        }
    }
    let invalid = |e: String| ConfigError::at(path, line, format!("invalid ast-grep rule: {e}"));
    let config: SerializableRuleConfig<SupportLang> =
        serde_json::from_value(serde_json::Value::Object(rule))
            .map_err(|e| invalid(e.to_string()))?;
    let language = config.language;
    let rule = RuleConfig::try_from(config, &GlobalRules::default())
        .map_err(|e| invalid(error_chain(&e)))?;
    Ok(UnitRule {
        language,
        matcher: rule.matcher,
        files,
        ignores,
    })
}

fn error_chain(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(e) = source {
        message.push_str(": ");
        message.push_str(&e.to_string());
        source = e.source();
    }
    message
}

/// A piece of code the question is asked about.
#[derive(Debug, Clone)]
pub struct Unit {
    pub file: String,
    /// Byte range in the file, which identifies the unit across decision checks.
    pub range: Range<usize>,
    pub line: usize,
    pub language: String,
    pub source: String,
    pub enclosing: Option<String>,
}

impl DecisionSpec {
    /// Whether any unit rule could select code in this file, `relative_path`
    /// being relative to the folder of the Reqfile.
    pub fn applies_to(&self, relative_path: &str) -> bool {
        self.rules_for(relative_path).next().is_some()
    }

    fn rules_for<'s>(&'s self, relative_path: &'s str) -> impl Iterator<Item = &'s UnitRule> + 's {
        let language = SupportLang::from_path(relative_path);
        self.units.iter().filter(move |rule| {
            Some(rule.language) == language
                && rule
                    .files
                    .as_ref()
                    .is_none_or(|g| g.is_match(relative_path))
                && !rule
                    .ignores
                    .as_ref()
                    .is_some_and(|g| g.is_match(relative_path))
        })
    }

    /// The units of one file, `file` being its repository path.
    pub fn units(&self, file: &str, relative_path: &str, source: &str) -> Vec<Unit> {
        let mut units: Vec<Unit> = Vec::new();
        for rule in self.rules_for(relative_path) {
            let grep = rule.language.ast_grep(source);
            for found in grep.root().find_all(&rule.matcher) {
                let range = found.range();
                if units.iter().any(|u| u.range == range) {
                    continue;
                }
                let enclosing = self
                    .enclosing
                    .then(|| {
                        found
                            .ancestors()
                            .find(|n| FUNCTION_KINDS.contains(&n.kind().as_ref()))
                    })
                    .flatten()
                    .map(|n| n.text().into_owned());
                units.push(Unit {
                    file: file.to_string(),
                    range,
                    line: found.start_pos().line() + 1,
                    language: rule.language.to_string().to_lowercase(),
                    source: found.text().into_owned(),
                    enclosing,
                });
            }
        }
        units.sort_by_key(|u| u.range.start);
        units
    }

    fn noul(&self, subject: &str) -> serde_json::Value {
        json!({
            "type": "noul",
            "instructions": { "subject": subject, "question": self.question },
            "criteria": { "true": self.violation_when, "false": self.ok_when },
        })
    }

    pub fn verdict(&self, probability: f64) -> Verdict {
        if probability > self.violation_above {
            Verdict::Violation
        } else if probability < self.pass_below {
            Verdict::Pass
        } else {
            Verdict::Uncertain
        }
    }

    /// Identifies this version of the question, its criteria and thresholds.
    pub fn fingerprint(&self) -> String {
        super::runlog::fingerprint(&format!(
            "{}\n{}\n{}\n{}\n{}",
            self.question, self.violation_when, self.ok_when, self.violation_above, self.pass_below
        ))
    }

    pub fn violation_message(&self) -> &str {
        &self.violation_when
    }

    pub fn question(&self) -> &str {
        &self.question
    }
}

#[derive(Debug, PartialEq)]
pub enum Verdict {
    Violation,
    Uncertain,
    Pass,
}

/// One Jev request: a unit, and the decision checks that selected it, as
/// (check index, unit index within that check).
pub struct Batch<'u> {
    pub unit: &'u Unit,
    pub members: Vec<(usize, usize)>,
}

/// Groups the units of every decision check so each piece of code is sent
/// once, with the questions of all the checks that selected it. Jev reads the
/// state once per request, so this is what makes several checks cheap.
pub fn batch<'u>(checks: &[&'u [Unit]]) -> Vec<Batch<'u>> {
    let mut batches: Vec<Batch<'u>> = Vec::new();
    let mut index: HashMap<(&str, usize, usize, bool), usize> = HashMap::new();
    for (check, units) in checks.iter().enumerate() {
        for (position, unit) in units.iter().enumerate() {
            // A unit sent with its enclosing function is a different state.
            let key = (
                unit.file.as_str(),
                unit.range.start,
                unit.range.end,
                unit.enclosing.is_some(),
            );
            match index.get(&key) {
                Some(&i) => batches[i].members.push((check, position)),
                None => {
                    index.insert(key, batches.len());
                    batches.push(Batch {
                        unit,
                        members: vec![(check, position)],
                    });
                }
            }
        }
    }
    batches
}

/// What Jev is told about a unit, and the question of each decision check
/// that selected it, keyed by an id unique in the request.
pub struct Questions {
    pub state: serde_json::Value,
    pub questions: Vec<(String, serde_json::Value)>,
}

/// The questions about one unit; `checks` pairs each question id (never sent
/// to the model) with its decision check.
pub fn questions(unit: &Unit, checks: &[(&str, &DecisionSpec)]) -> Questions {
    let mut state = json!({ "language": unit.language, "path": unit.file, "unit": unit.source });
    let subject = match &unit.enclosing {
        Some(enclosing) => {
            state["enclosing_function"] = json!(enclosing);
            "the code in `unit`, which is part of `enclosing_function`"
        }
        None => "the code in `unit`",
    };
    let questions = checks
        .iter()
        .map(|(id, spec)| (id.to_string(), spec.noul(subject)))
        .collect();
    Questions { state, questions }
}

/// The questions about one unit, with the cache key of each.
pub struct Asked {
    pub questions: Questions,
    pub keys: Vec<String>,
}

/// What to ask `model` about a unit on behalf of `checks`.
pub fn ask(unit: &Unit, checks: &[(&str, &DecisionSpec)], model: &str) -> Asked {
    let questions = questions(unit, checks);
    let keys = questions
        .questions
        .iter()
        .map(|(_, q)| cache_key(model, &questions.state, q))
        .collect();
    Asked { questions, keys }
}

/// Hands each batch's results back to the checks it came from: `sizes` is
/// the number of units of each check, and the result has one entry per unit.
pub fn spread<T>(batches: &[Batch], sizes: &[usize], per_batch: Vec<Vec<T>>) -> Vec<Vec<T>> {
    let mut by_check: Vec<Vec<Option<T>>> = sizes
        .iter()
        .map(|&n| (0..n).map(|_| None).collect())
        .collect();
    for (batch, results) in batches.iter().zip(per_batch) {
        for (&(check, unit), result) in batch.members.iter().zip(results) {
            by_check[check][unit] = Some(result);
        }
    }
    by_check
        .into_iter()
        .map(|units| {
            units
                .into_iter()
                .map(|r| r.expect("every unit belongs to a batch"))
                .collect()
        })
        .collect()
}

/// Identifies one question about one state asked to one exact model,
/// whatever request carries it. 64 bits keep collisions out of reach for a
/// repository's worth of answers while halving the cache size.
pub fn cache_key(model: &str, state: &serde_json::Value, question: &serde_json::Value) -> String {
    let digest = Sha256::digest(format!("{model}\n{state}\n{question}"));
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// The request asking the given questions about a state.
pub fn request(
    model: &str,
    state: &serde_json::Value,
    questions: &[&(String, serde_json::Value)],
) -> serde_json::Value {
    let questions: serde_json::Map<String, serde_json::Value> = questions
        .iter()
        .map(|(id, q)| (id.clone(), q.clone()))
        .collect();
    json!({ "model": model, "state": state, "questions": questions })
}

/// A request of a few tokens whose answer names the model `model` resolves to.
pub fn probe_request(model: &str) -> serde_json::Value {
    json!({
        "model": model,
        "state": "reqfile",
        "questions": { "probe": { "type": "noul", "instructions": "Is this text empty?" } },
    })
}

/// A Jev response: the model that answered, and the probability of a
/// violation for each question id, in order.
pub fn parse_response(body: &str, ids: &[&str]) -> Result<(String, Vec<f64>), String> {
    let response: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("Jev returned invalid JSON: {e}"))?;
    let model = response["model"]
        .as_str()
        .ok_or_else(|| format!("Jev response has no model: {}", abbreviate(body)))?
        .to_string();
    let probabilities = ids
        .iter()
        .map(|id| match response["answers"][id]["noul"].as_f64() {
            Some(p) if (0.0..=1.0).contains(&p) => Ok(p),
            _ => Err(format!(
                "Jev response has no probability for {id}: {}",
                abbreviate(body)
            )),
        })
        .collect::<Result<_, _>>()?;
    Ok((model, probabilities))
}

/// Why a unit cannot be sent to Jev whole, if it cannot.
pub fn too_large(unit: &Unit) -> Option<String> {
    let size =
        unit.source.chars().count() + unit.enclosing.as_ref().map_or(0, |e| e.chars().count());
    (size > MAX_SOURCE_CHARS).then(|| {
        format!("the unit is too large for Jev to judge ({size} characters of code, at most {MAX_SOURCE_CHARS}); split it")
    })
}

fn abbreviate(text: &str) -> String {
    let text = text.trim();
    match text.char_indices().nth(300) {
        None => text.to_string(),
        Some((cut, _)) => format!("{}…", &text[..cut]),
    }
}
