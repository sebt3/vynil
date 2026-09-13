pub mod config;
pub mod hbs_checker;
pub mod rhai_checker;

use junit_report::{ReportBuilder, TestCase, TestSuiteBuilder};
use serde::{Deserialize, Serialize};
use std::{fmt::Write as _, path::PathBuf, str::FromStr};

pub use config::{LintConfig, parse_inline_disables};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Copy, Serialize, Deserialize)]
pub enum LintLevel {
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for LintLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Error => write!(f, "ERROR"),
            Self::Warn => write!(f, "WARN"),
            Self::Info => write!(f, "INFO"),
        }
    }
}

impl FromStr for LintLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "error" => Ok(Self::Error),
            "warn" => Ok(Self::Warn),
            "info" => Ok(Self::Info),
            _ => Err(format!("Unknown lint level: {s}")),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LintFinding {
    pub rule: String,
    pub level: LintLevel,
    pub file: PathBuf,
    pub line: Option<usize>,
    pub message: String,
}

pub struct LintResultCollector {
    findings: Vec<LintFinding>,
}

impl LintResultCollector {
    pub const fn new() -> Self {
        Self { findings: Vec::new() }
    }

    pub fn add(&mut self, finding: LintFinding) {
        self.findings.push(finding);
    }

    pub fn extend(&mut self, findings: Vec<LintFinding>) {
        self.findings.extend(findings);
    }

    pub fn has_errors(&self) -> bool {
        self.findings.iter().any(|f| f.level == LintLevel::Error)
    }

    pub fn prefix_files(&mut self, base: &std::path::Path) {
        for finding in &mut self.findings {
            if finding.file.is_relative() {
                finding.file = base.join(&finding.file);
            }
        }
    }

    pub fn to_text(&self, level_filter: LintLevel) -> String {
        let mut out = String::new();
        let filtered: Vec<_> = self.findings.iter().filter(|f| f.level >= level_filter).collect();

        for finding in filtered {
            let file_str = finding.file.display().to_string();
            let location = match finding.line {
                Some(l) => format!("{file_str}:{l}"),
                None => file_str,
            };

            let _ = writeln!(out, "[{:<5}] {}  {}", finding.level, finding.rule, location);
            let _ = writeln!(out, "        {}", finding.message);
            out.push('\n');
        }

        let error_count = self
            .findings
            .iter()
            .filter(|f| f.level == LintLevel::Error)
            .count();
        let warn_count = self
            .findings
            .iter()
            .filter(|f| f.level == LintLevel::Warn)
            .count();
        let _ = writeln!(
            out,
            "Résultat : {} erreur{}, {} warning",
            error_count,
            if error_count == 1 { "" } else { "s" },
            warn_count
        );

        out
    }

    pub fn to_json(&self, level_filter: LintLevel) -> String {
        let filtered: Vec<_> = self.findings.iter().filter(|f| f.level >= level_filter).collect();
        let error_count = self
            .findings
            .iter()
            .filter(|f| f.level == LintLevel::Error)
            .count();
        let warn_count = self
            .findings
            .iter()
            .filter(|f| f.level == LintLevel::Warn)
            .count();
        let output = serde_json::json!({
            "findings": filtered,
            "summary": { "errors": error_count, "warnings": warn_count }
        });
        serde_json::to_string_pretty(&output).unwrap_or_default()
    }

    /// # Panics
    ///
    /// Panics if the `JUnit` XML report cannot be written or is not valid UTF-8 (cannot happen
    /// for in-memory finding data).
    pub fn to_junit(&self) -> String {
        let mut report_builder = ReportBuilder::new();
        let mut suites: std::collections::BTreeMap<String, Vec<&LintFinding>> =
            std::collections::BTreeMap::new();

        for finding in &self.findings {
            let category = finding.rule.split('/').next().unwrap_or("unknown").to_string();
            suites.entry(category).or_default().push(finding);
        }

        for (category, findings) in suites {
            let mut suite = TestSuiteBuilder::new(&category);
            for finding in findings {
                let tc = TestCase::failure(
                    &finding.rule,
                    junit_report::Duration::seconds(0),
                    "lint",
                    &finding.message,
                );
                suite.add_testcase(tc);
            }
            report_builder.add_testsuite(suite.build());
        }

        let report = report_builder.build();
        let mut buf: Vec<u8> = Vec::new();
        if let Err(e) = report.write_xml(&mut buf) {
            panic!("failed to write JUnit XML: {e}");
        }
        match String::from_utf8(buf) {
            Ok(text) => text,
            Err(e) => panic!("JUnit XML is not valid UTF-8: {e}"),
        }
    }
}

/// Scan raw YAML text to find the line number (1-indexed) of each direct key under a given section.
pub fn find_section_key_line_numbers(
    yaml_path: &std::path::Path,
    section: &str,
) -> std::collections::BTreeMap<String, usize> {
    let mut map = std::collections::BTreeMap::new();
    let Ok(content) = std::fs::read_to_string(yaml_path) else {
        return map;
    };

    let section_header = format!("{section}:");
    let mut in_section = false;
    let mut section_indent = 0_usize;
    let mut key_indent: Option<usize> = None;

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len().saturating_sub(trimmed.len());

        if in_section {
            let sec_indent = section_indent;
            if indent <= sec_indent {
                break;
            }
            if key_indent.is_none() {
                key_indent = Some(indent);
            }
            if Some(indent) == key_indent
                && let Some(key) = trimmed.split(':').next()
            {
                let key = key.trim().to_string();
                if !key.is_empty() {
                    map.insert(key, i.saturating_add(1));
                }
            }
        } else if trimmed == section_header || trimmed.starts_with(&format!("{section_header} ")) {
            in_section = true;
            section_indent = indent;
        }
    }
    map
}

/// Scan raw YAML text to find the line number (1-indexed) of each key under `options:`.
pub fn find_option_line_numbers(yaml_path: &std::path::Path) -> std::collections::BTreeMap<String, usize> {
    find_section_key_line_numbers(yaml_path, "options")
}

impl Default for LintResultCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collector_has_no_errors_when_empty() {
        let collector = LintResultCollector::new();
        assert!(!collector.has_errors());
    }

    #[test]
    fn collector_detects_error_level() {
        let mut collector = LintResultCollector::new();
        collector.add(LintFinding {
            rule: "test/rule".to_string(),
            level: LintLevel::Error,
            file: PathBuf::from("test.txt"),
            line: Some(1),
            message: "Test error".to_string(),
        });
        assert!(collector.has_errors());
    }

    #[test]
    fn to_text_filters_by_level() {
        let mut collector = LintResultCollector::new();
        collector.add(LintFinding {
            rule: "test/error".to_string(),
            level: LintLevel::Error,
            file: PathBuf::from("test.txt"),
            line: Some(1),
            message: "Error message".to_string(),
        });
        collector.add(LintFinding {
            rule: "test/warn".to_string(),
            level: LintLevel::Warn,
            file: PathBuf::from("test.txt"),
            line: Some(2),
            message: "Warn message".to_string(),
        });

        let text = collector.to_text(LintLevel::Error);
        assert!(text.contains("test/error"));
        assert!(!text.contains("test/warn"));
    }

    #[test]
    fn to_junit_groups_by_category() {
        let mut collector = LintResultCollector::new();
        collector.add(LintFinding {
            rule: "hbs/foo".to_string(),
            level: LintLevel::Error,
            file: PathBuf::from("test.hbs"),
            line: Some(1),
            message: "HBS error".to_string(),
        });
        collector.add(LintFinding {
            rule: "rhai/bar".to_string(),
            level: LintLevel::Error,
            file: PathBuf::from("test.rhai"),
            line: Some(1),
            message: "Rhai error".to_string(),
        });

        let junit = collector.to_junit();
        assert!(junit.contains("hbs"));
        assert!(junit.contains("rhai"));
        assert!(junit.contains("hbs/foo"));
        assert!(junit.contains("rhai/bar"));
    }
}
