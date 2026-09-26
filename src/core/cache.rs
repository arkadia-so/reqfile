//! Jev answers already known, so unchanged code is not asked about again.
//!
//! One answer per line, `<key> <probability>`, sorted.

use std::collections::{BTreeMap, BTreeSet};

pub const FILE_NAME: &str = "jev-cache";

#[derive(Default)]
pub struct Cache {
    answers: BTreeMap<String, f64>,
    used: BTreeSet<String>,
}

impl Cache {
    /// Reads a cache; a duplicate key keeps its last answer.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut answers = BTreeMap::new();
        for (number, line) in text
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty())
        {
            let invalid = || format!("invalid line {}: {line:?}", number + 1);
            let (key, probability) = line.split_once(' ').ok_or_else(invalid)?;
            let probability: f64 = probability.trim().parse().map_err(|_| invalid())?;
            if key.is_empty() || !(0.0..=1.0).contains(&probability) {
                return Err(invalid());
            }
            answers.insert(key.to_string(), probability);
        }
        Ok(Self {
            answers,
            used: BTreeSet::new(),
        })
    }

    /// The cache as written: every answer, or with `prune` only those used in this run.
    pub fn render(&self, prune: bool) -> String {
        self.answers
            .iter()
            .filter(|(key, _)| !prune || self.used.contains(*key))
            .map(|(key, p)| format!("{key} {p}\n"))
            .collect()
    }

    pub fn get(&mut self, key: &str) -> Option<f64> {
        let answer = self.answers.get(key).copied();
        if answer.is_some() {
            self.used.insert(key.to_string());
        }
        answer
    }

    pub fn insert(&mut self, key: String, probability: f64) {
        self.used.insert(key.clone());
        self.answers.insert(key, probability);
    }
}
