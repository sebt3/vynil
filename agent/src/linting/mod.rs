pub mod config;

use std::path::PathBuf;
use std::str::FromStr;
use junit_report::{ReportBuilder, TestCase, TestSuiteBuilder};
use serde::{Deserialize, Serialize};

pub use config::{LintConfig, FileRule, parse_inline_disables};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Copy, Serialize, Deserialize)]
pub enum LintLevel {
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for LintLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LintLevel::Error => write!(f, "ERROR"),
            LintLevel::Warn => write!(f, "WARN"),
            LintLevel::Info => write!(f, "INFO"),
        }
    }
}

impl FromStr for LintLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "error" => Ok(LintLevel::Error),
            "warn" => Ok(LintLevel::Warn),
            "info" => Ok(LintLevel::Info),
            _ => Err(format!("Unknown lint level: {}", s)),
        }
    }
}

#[derive(Clone, Debug)]
pub struct LintFinding {
    pub rule: String,
    pub level: LintLevel,
    pub file: PathBuf,
    pub line: Option<usize>,
    pub col: Option<usize>,
    pub message: String,
}

pub struct LintResultCollector {
    findings: Vec<LintFinding>,
}

impl LintResultCollector {
    pub fn new() -> Self {
        Self {
            findings: Vec::new(),
        }
    }

    pub fn add(&mut self, finding: LintFinding) {
        self.findings.push(finding);
    }

    pub fn has_errors(&self) -> bool {
        self.findings.iter().any(|f| f.level == LintLevel::Error)
    }

    pub fn has_warnings(&self) -> bool {
        self.findings
            .iter()
            .any(|f| f.level == LintLevel::Warn || f.level == LintLevel::Error)
    }

    pub fn to_text(&self, level_filter: LintLevel) -> String {
        let mut out = String::new();
        let filtered: Vec<_> = self
            .findings
            .iter()
            .filter(|f| f.level >= level_filter)
            .collect();

        for finding in filtered {
            let file_str = finding.file.display().to_string();
            let location = match (finding.line, finding.col) {
                (Some(l), Some(c)) => format!("{}:{}:{}", file_str, l, c),
                (Some(l), None) => format!("{}:{}", file_str, l),
                _ => file_str,
            };

            out.push_str(&format!(
                "[{:<5}] {}  {}\n",
                finding.level, finding.rule, location
            ));
            out.push_str(&format!("        {}\n", finding.message));
            out.push('\n');
        }

        let error_count = self.findings.iter().filter(|f| f.level == LintLevel::Error).count();
        let warn_count = self.findings.iter().filter(|f| f.level == LintLevel::Warn).count();
        out.push_str(&format!(
            "Résultat : {} erreur{}, {} warning",
            error_count,
            if error_count != 1 { "s" } else { "" },
            warn_count
        ));

        out
    }

    pub fn to_junit(&self) -> String {
        let mut report_builder = ReportBuilder::new();
        let mut suites: std::collections::BTreeMap<String, Vec<&LintFinding>> =
            std::collections::BTreeMap::new();

        for finding in &self.findings {
            let category = finding
                .rule
                .split('/')
                .next()
                .unwrap_or("unknown")
                .to_string();
            suites.entry(category).or_insert_with(Vec::new).push(finding);
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
        report
            .write_xml(&mut buf)
            .expect("failed to write JUnit XML");
        String::from_utf8(buf).expect("JUnit XML is not valid UTF-8")
    }
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
        assert!(!collector.has_warnings());
    }

    #[test]
    fn collector_detects_error_level() {
        let mut collector = LintResultCollector::new();
        collector.add(LintFinding {
            rule: "test/rule".to_string(),
            level: LintLevel::Error,
            file: PathBuf::from("test.txt"),
            line: Some(1),
            col: Some(5),
            message: "Test error".to_string(),
        });
        assert!(collector.has_errors());
        assert!(collector.has_warnings());
    }

    #[test]
    fn to_text_filters_by_level() {
        let mut collector = LintResultCollector::new();
        collector.add(LintFinding {
            rule: "test/error".to_string(),
            level: LintLevel::Error,
            file: PathBuf::from("test.txt"),
            line: Some(1),
            col: None,
            message: "Error message".to_string(),
        });
        collector.add(LintFinding {
            rule: "test/warn".to_string(),
            level: LintLevel::Warn,
            file: PathBuf::from("test.txt"),
            line: Some(2),
            col: None,
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
            col: None,
            message: "HBS error".to_string(),
        });
        collector.add(LintFinding {
            rule: "rhai/bar".to_string(),
            level: LintLevel::Error,
            file: PathBuf::from("test.rhai"),
            line: Some(1),
            col: None,
            message: "Rhai error".to_string(),
        });

        let junit = collector.to_junit();
        assert!(junit.contains("hbs"));
        assert!(junit.contains("rhai"));
        assert!(junit.contains("hbs/foo"));
        assert!(junit.contains("rhai/bar"));
    }
}
