//! The run log: one JSON line per judged code unit, command check and
//! command finding, so decisions can be harvested, labeled and replayed.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// What every line of one run shares.
pub struct Context {
    pub run_id: String,
    pub time_ms: u128,
    pub git_head: Option<String>,
    pub fast: bool,
    /// With `--changed`, its base, or `null` when the default applied.
    pub changed: Option<Option<String>>,
    pub tags: BTreeMap<String, String>,
}

impl Context {
    /// A line of kind `kind` with its own fields, after the shared ones.
    pub fn line(&self, kind: &str, fields: Value) -> Value {
        let mut line = json!({
            "v": 1,
            "kind": kind,
            "run_id": self.run_id,
            "time_ms": self.time_ms,
            "git_head": self.git_head,
            "reqfile_version": env!("CARGO_PKG_VERSION"),
            "fast": self.fast,
            "changed": self.changed.is_some(),
            "base": self.changed.clone().flatten(),
            "tags": self.tags,
        });
        if let (Some(line), Value::Object(fields)) = (line.as_object_mut(), fields) {
            line.extend(fields);
        }
        line
    }
}

/// A short, stable fingerprint of some text.
pub fn fingerprint(text: &str) -> String {
    Sha256::digest(text)[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Parses `key=value` tags.
pub fn parse_tags(tags: &[String]) -> Result<BTreeMap<String, String>, String> {
    tags.iter()
        .map(|tag| match tag.split_once('=') {
            Some((key, value)) if !key.is_empty() => Ok((key.to_string(), value.to_string())),
            _ => Err(format!("--log-tag must be KEY=VALUE, got `{tag}`")),
        })
        .collect()
}
