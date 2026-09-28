//! The subset of JUnit XML that reqfile reads: each test case, and whether
//! it failed. Test runners write it (cargo-nextest, pytest, vitest, bun, go
//! with gotestsum), so a failing test becomes a finding named after it.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use super::sarif;

/// A test that failed or errored.
#[derive(Debug, PartialEq)]
pub struct Failure {
    /// `classname::name`, or `name` alone without a class.
    pub test: String,
    /// The first line of the failure message, if the runner gave one.
    pub message: Option<String>,
    pub file: Option<String>,
    pub line: Option<usize>,
}

pub struct Parsed {
    /// Whether the report lists any test case, failed or not.
    pub had_tests: bool,
    pub failures: Vec<Failure>,
}

/// Parses a JUnit report written by a command run from repository folder
/// `cwd`; `file` attributes become repository paths.
pub fn parse(text: &str, cwd: &str, repo_root: &str) -> Result<Parsed, String> {
    let mut reader = Reader::from_str(text);
    let mut had_tests = false;
    let mut failures = Vec::new();
    // The test case being read, and whether it failed.
    let mut current: Option<(Failure, bool)> = None;
    let mut saw_root = false;
    loop {
        let event = reader
            .read_event()
            .map_err(|e| format!("output is not valid JUnit XML: {e}"))?;
        match event {
            Event::Start(tag) | Event::Empty(tag)
                if matches!(tag.name().as_ref(), b"testsuites" | b"testsuite") =>
            {
                saw_root = true;
            }
            Event::Start(tag) if tag.name().as_ref() == b"testcase" => {
                had_tests = true;
                current = Some((case(&tag, cwd, repo_root)?, false));
            }
            Event::Empty(tag) if tag.name().as_ref() == b"testcase" => {
                had_tests = true;
            }
            Event::Start(tag) | Event::Empty(tag)
                if matches!(tag.name().as_ref(), b"failure" | b"error") =>
            {
                if let Some((failure, failed)) = current.as_mut() {
                    *failed = true;
                    if failure.message.is_none() {
                        failure.message = attribute(&tag, b"message")?
                            .and_then(|m| m.lines().next().map(str::to_string))
                            .filter(|m| !m.trim().is_empty());
                    }
                }
            }
            Event::End(tag) if tag.name().as_ref() == b"testcase" => {
                if let Some((failure, true)) = current.take() {
                    failures.push(failure);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !saw_root {
        return Err("output is not a JUnit report: no <testsuites> or <testsuite>".into());
    }
    Ok(Parsed {
        had_tests,
        failures,
    })
}

fn case(tag: &BytesStart, cwd: &str, repo_root: &str) -> Result<Failure, String> {
    let name = attribute(tag, b"name")?.unwrap_or_default();
    let test = match attribute(tag, b"classname")? {
        Some(class) if !class.is_empty() => format!("{class}::{name}"),
        _ => name,
    };
    let file = attribute(tag, b"file")?.map(|f| sarif::repository_path(&f, cwd, repo_root));
    let line = attribute(tag, b"line")?.and_then(|l| l.parse().ok());
    Ok(Failure {
        test,
        message: None,
        file,
        line,
    })
}

fn attribute(tag: &BytesStart, key: &[u8]) -> Result<Option<String>, String> {
    for attribute in tag.attributes() {
        let attribute = attribute.map_err(|e| format!("output is not valid JUnit XML: {e}"))?;
        if attribute.key.as_ref() == key {
            let value = attribute
                .unescape_value()
                .map_err(|e| format!("output is not valid JUnit XML: {e}"))?;
            return Ok(Some(value.into_owned()));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites>
  <testsuite name="output" tests="3">
    <testcase classname="output" name="passes"/>
    <testcase classname="output" name="names_conflicts" file="tests/output.rs" line="12">
      <failure message="assertion failed: breaks&#10;second line">details</failure>
    </testcase>
    <testcase name="crashes"><error/></testcase>
  </testsuite>
</testsuites>"#;

    #[test]
    fn failed_and_errored_cases_are_failures_named_after_their_test() {
        let parsed = parse(REPORT, "", "/repo").expect("valid JUnit");
        assert!(parsed.had_tests);
        assert_eq!(
            parsed.failures,
            [
                Failure {
                    test: "output::names_conflicts".into(),
                    message: Some("assertion failed: breaks".into()),
                    file: Some("tests/output.rs".into()),
                    line: Some(12),
                },
                Failure {
                    test: "crashes".into(),
                    message: None,
                    file: None,
                    line: None,
                },
            ]
        );
    }

    #[test]
    fn a_report_without_a_suite_is_an_error() {
        assert!(parse("<html/>", "", "/repo").is_err());
        assert!(parse("not xml <", "", "/repo").is_err());
    }
}
