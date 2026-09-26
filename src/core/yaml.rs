//! A YAML document as a tree of nodes that remember their line, so every
//! error can name the file and line it comes from.

use saphyr::Scalar;
use saphyr_parser::{Event, Parser, Span};

use super::error::ConfigError;

#[derive(Debug, Clone)]
pub struct Node {
    pub line: usize,
    pub value: Value,
}

#[derive(Debug, Clone)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Seq(Vec<Node>),
    Map(Vec<(Key, Node)>),
}

#[derive(Debug, Clone)]
pub struct Key {
    pub name: String,
    pub line: usize,
}

/// Parses a single YAML document. An empty document is `Null` at line 1.
pub fn parse(file: &str, text: &str) -> Result<Node, ConfigError> {
    let mut events = Vec::new();
    for event in Parser::new_from_str(text) {
        match event {
            Ok((event, span)) => events.push((event, span)),
            Err(e) => return Err(ConfigError::at(file, e.marker().line(), e.info())),
        }
    }
    let mut builder = Builder {
        file,
        events: events.into_iter().peekable(),
    };
    builder.document()
}

struct Builder<'a, I: Iterator<Item = (Event<'a>, Span)>> {
    file: &'a str,
    events: std::iter::Peekable<I>,
}

impl<'a, I: Iterator<Item = (Event<'a>, Span)>> Builder<'a, I> {
    fn document(&mut self) -> Result<Node, ConfigError> {
        let mut root = None;
        while let Some((event, span)) = self.events.next() {
            match event {
                Event::StreamStart | Event::DocumentEnd | Event::StreamEnd | Event::Nothing => {}
                Event::DocumentStart(_) if root.is_some() => {
                    return Err(ConfigError::at(
                        self.file,
                        span.start.line(),
                        "expected a single YAML document",
                    ));
                }
                Event::DocumentStart(_) => root = Some(self.node()?),
                _ => {
                    return Err(ConfigError::at(
                        self.file,
                        span.start.line(),
                        "unexpected YAML event",
                    ));
                }
            }
        }
        Ok(root.unwrap_or(Node {
            line: 1,
            value: Value::Null,
        }))
    }

    fn node(&mut self) -> Result<Node, ConfigError> {
        let Some((event, span)) = self.events.next() else {
            return Err(ConfigError::at(self.file, 1, "unexpected end of YAML"));
        };
        let line = span.start.line();
        let value = match event {
            Event::Scalar(text, style, _, tag) => {
                match Scalar::parse_from_cow_and_metadata(text, style, tag.as_ref()) {
                    Some(Scalar::Null) => Value::Null,
                    Some(Scalar::Boolean(b)) => Value::Bool(b),
                    Some(Scalar::Integer(i)) => Value::Int(i),
                    Some(Scalar::FloatingPoint(f)) => Value::Float(f.into_inner()),
                    Some(Scalar::String(s)) => Value::Str(s.into_owned()),
                    None => {
                        return Err(ConfigError::at(
                            self.file,
                            line,
                            "invalid scalar for its tag",
                        ));
                    }
                }
            }
            Event::SequenceStart(..) => {
                let mut items = Vec::new();
                while !matches!(self.events.peek(), Some((Event::SequenceEnd, _))) {
                    items.push(self.node()?);
                }
                self.events.next();
                Value::Seq(items)
            }
            Event::MappingStart(..) => {
                let mut entries: Vec<(Key, Node)> = Vec::new();
                while !matches!(self.events.peek(), Some((Event::MappingEnd, _))) {
                    let key = self.node()?;
                    let name = match key.value {
                        Value::Str(s) => s,
                        Value::Int(i) => i.to_string(),
                        Value::Bool(b) => b.to_string(),
                        _ => {
                            return Err(ConfigError::at(
                                self.file,
                                key.line,
                                "mapping keys must be plain strings",
                            ));
                        }
                    };
                    if let Some((previous, _)) = entries.iter().find(|(k, _)| k.name == name) {
                        return Err(ConfigError::at(
                            self.file,
                            key.line,
                            format!(
                                "duplicate key `{name}` (first defined at line {})",
                                previous.line
                            ),
                        ));
                    }
                    let value = self.node()?;
                    entries.push((
                        Key {
                            name,
                            line: key.line,
                        },
                        value,
                    ));
                }
                self.events.next();
                Value::Map(entries)
            }
            Event::Alias(_) => {
                return Err(ConfigError::at(
                    self.file,
                    line,
                    "YAML aliases are not supported",
                ));
            }
            _ => return Err(ConfigError::at(self.file, line, "unexpected YAML event")),
        };
        Ok(Node { line, value })
    }
}

impl Node {
    pub fn describe(&self) -> &'static str {
        match self.value {
            Value::Null => "null",
            Value::Bool(_) => "a boolean",
            Value::Int(_) => "an integer",
            Value::Float(_) => "a number",
            Value::Str(_) => "a string",
            Value::Seq(_) => "a list",
            Value::Map(_) => "a mapping",
        }
    }

    fn expected(&self, file: &str, what: &str, expected: &str) -> ConfigError {
        ConfigError::at(
            file,
            self.line,
            format!("`{what}` must be {expected}, found {}", self.describe()),
        )
    }

    /// A non-empty string.
    pub fn text(self, file: &str, what: &str) -> Result<String, ConfigError> {
        match self.value {
            Value::Str(s) if !s.trim().is_empty() => Ok(s),
            _ => Err(self.expected(file, what, "a non-empty string")),
        }
    }

    pub fn boolean(self, file: &str, what: &str) -> Result<bool, ConfigError> {
        match self.value {
            Value::Bool(b) => Ok(b),
            _ => Err(self.expected(file, what, "true or false")),
        }
    }

    pub fn integer(self, file: &str, what: &str) -> Result<i64, ConfigError> {
        match self.value {
            Value::Int(i) => Ok(i),
            _ => Err(self.expected(file, what, "an integer")),
        }
    }

    pub fn number(self, file: &str, what: &str) -> Result<f64, ConfigError> {
        match self.value {
            Value::Int(i) => Ok(i as f64),
            Value::Float(f) => Ok(f),
            _ => Err(self.expected(file, what, "a number")),
        }
    }

    pub fn list(self, file: &str, what: &str) -> Result<Vec<Node>, ConfigError> {
        match self.value {
            Value::Seq(items) => Ok(items),
            _ => Err(self.expected(file, what, "a list")),
        }
    }

    /// The entries of a mapping, which must only use the `allowed` keys.
    pub fn fields<'f>(
        self,
        file: &'f str,
        what: &str,
        allowed: &[&str],
    ) -> Result<Fields<'f>, ConfigError> {
        let Value::Map(entries) = self.value else {
            return Err(self.expected(file, what, "a mapping"));
        };
        if let Some((key, _)) = entries
            .iter()
            .find(|(k, _)| !allowed.contains(&k.name.as_str()))
        {
            return Err(ConfigError::at(
                file,
                key.line,
                format!(
                    "unknown key `{}` in {what} (expected one of: {})",
                    key.name,
                    allowed.join(", ")
                ),
            ));
        }
        Ok(Fields {
            file,
            line: self.line,
            what: what.to_string(),
            entries,
        })
    }

    /// The node as JSON, for handing opaque sub-documents (ast-grep rules) to serde.
    pub fn into_json(self) -> serde_json::Value {
        use serde_json::Value as J;
        match self.value {
            Value::Null => J::Null,
            Value::Bool(b) => J::Bool(b),
            Value::Int(i) => J::from(i),
            Value::Float(f) => J::from(f),
            Value::Str(s) => J::String(s),
            Value::Seq(items) => J::Array(items.into_iter().map(Node::into_json).collect()),
            Value::Map(entries) => J::Object(
                entries
                    .into_iter()
                    .map(|(k, v)| (k.name, v.into_json()))
                    .collect(),
            ),
        }
    }
}

/// The entries of a mapping, taken field by field.
pub struct Fields<'f> {
    file: &'f str,
    pub line: usize,
    pub what: String,
    entries: Vec<(Key, Node)>,
}

impl Fields<'_> {
    pub fn optional(&mut self, key: &str) -> Option<Node> {
        let index = self.entries.iter().position(|(k, _)| k.name == key)?;
        Some(self.entries.remove(index).1)
    }

    pub fn required(&mut self, key: &str) -> Result<Node, ConfigError> {
        self.optional(key).ok_or_else(|| {
            ConfigError::at(
                self.file,
                self.line,
                format!("{} is missing the required field `{key}`", self.what),
            )
        })
    }
}
