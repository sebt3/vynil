use super::vyniltestset::VynilAssertResult;
use junit_report::{Duration, ReportBuilder, TestCase, TestSuiteBuilder};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct TestResult {
    pub test_name: String,
    pub asserts: Vec<VynilAssertResult>,
    #[serde(serialize_with = "serialize_duration")]
    pub duration: std::time::Duration,
}

fn serialize_duration<S>(d: &std::time::Duration, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    s.serialize_f64(d.as_secs_f64())
}

#[derive(Clone, Debug)]
pub struct TestResultCollector {
    results: Vec<TestResult>,
}

impl TestResultCollector {
    #[must_use]
    pub const fn new() -> Self {
        Self { results: Vec::new() }
    }

    pub fn add(&mut self, test_name: String, asserts: Vec<VynilAssertResult>, duration: std::time::Duration) {
        self.results.push(TestResult {
            test_name,
            asserts,
            duration,
        });
    }

    #[must_use]
    pub const fn total_tests(&self) -> usize {
        self.results.len()
    }

    #[must_use]
    pub fn total_asserts(&self) -> usize {
        self.results.iter().map(|r| r.asserts.len()).sum()
    }

    #[must_use]
    pub fn total_passed(&self) -> usize {
        self.results
            .iter()
            .flat_map(|r| &r.asserts)
            .filter(|a| a.passed)
            .count()
    }

    #[must_use]
    pub fn total_failed(&self) -> usize {
        self.results
            .iter()
            .flat_map(|r| &r.asserts)
            .filter(|a| !a.passed)
            .count()
    }

    #[must_use]
    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.asserts.iter().all(|a| a.passed))
    }

    #[must_use]
    pub fn to_text(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        for result in &self.results {
            let _ = writeln!(
                out,
                "Test: {} ({:.3}s)",
                result.test_name,
                result.duration.as_secs_f64()
            );
            for a in &result.asserts {
                let status = if a.passed { "PASS" } else { "FAIL" };
                match &a.description {
                    Some(desc) if !desc.is_empty() => {
                        let _ = writeln!(out, "  [{status}] {} ({desc}): {}", a.name, a.message);
                    }
                    _ => {
                        let _ = writeln!(out, "  [{status}] {}: {}", a.name, a.message);
                    }
                }
            }
            out.push('\n');
        }
        let _ = writeln!(
            out,
            "Results: {} passed, {} failed, {} total",
            self.total_passed(),
            self.total_failed(),
            self.total_asserts()
        );
        out
    }

    #[must_use]
    pub fn to_json(&self) -> String {
        let output = serde_json::json!({
            "tests": self.results,
            "summary": {
                "total_tests": self.total_tests(),
                "total_asserts": self.total_asserts(),
                "passed": self.total_passed(),
                "failed": self.total_failed()
            }
        });
        serde_json::to_string_pretty(&output).unwrap_or_default()
    }

    /// # Panics
    ///
    /// Panics if the `JUnit` XML report cannot be written into the in-memory buffer or is not
    /// valid UTF-8 (neither can happen for in-memory `String` test data).
    #[must_use]
    pub fn to_junit(&self) -> String {
        let mut report_builder = ReportBuilder::new();
        for result in &self.results {
            let mut suite = TestSuiteBuilder::new(&result.test_name);
            let nanos = result.duration.as_nanos();
            let assert_count = u128::try_from(result.asserts.len().max(1)).unwrap_or(1);
            let per_assert_nanos = nanos.checked_div(assert_count).unwrap_or(0);
            let secs = i64::try_from(per_assert_nanos / 1_000_000_000).unwrap_or(i64::MAX);
            let nanos_part = i32::try_from(per_assert_nanos % 1_000_000_000).unwrap_or(i32::MAX);
            let tc_dur = Duration::new(secs, nanos_part);
            for a in &result.asserts {
                let tc = if a.passed {
                    TestCase::success(&a.name, tc_dur)
                } else {
                    TestCase::failure(&a.name, tc_dur, "assertion", &a.message)
                };
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

impl Default for TestResultCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::VynilAssertResult;

    fn passing_assert(name: &str) -> VynilAssertResult {
        VynilAssertResult {
            name: name.to_string(),
            description: None,
            passed: true,
            message: "ok".to_string(),
        }
    }

    fn failing_assert(name: &str) -> VynilAssertResult {
        VynilAssertResult {
            name: name.to_string(),
            description: None,
            passed: false,
            message: "no match".to_string(),
        }
    }

    #[test]
    fn all_passed_empty_collector() {
        let c = TestResultCollector::new();
        assert!(c.all_passed());
    }

    #[test]
    fn all_passed_when_all_asserts_pass() {
        let mut c = TestResultCollector::new();
        c.add(
            "t1".to_string(),
            vec![passing_assert("a"), passing_assert("b")],
            std::time::Duration::ZERO,
        );
        assert!(c.all_passed());
    }

    #[test]
    fn all_passed_false_when_one_assert_fails() {
        let mut c = TestResultCollector::new();
        c.add(
            "t1".to_string(),
            vec![passing_assert("a"), failing_assert("b")],
            std::time::Duration::ZERO,
        );
        assert!(!c.all_passed());
    }

    #[test]
    fn all_passed_false_when_all_asserts_fail() {
        let mut c = TestResultCollector::new();
        c.add(
            "t1".to_string(),
            vec![failing_assert("a"), failing_assert("b")],
            std::time::Duration::ZERO,
        );
        assert!(!c.all_passed());
    }

    #[test]
    fn total_failed_counts_only_failed() {
        let mut c = TestResultCollector::new();
        c.add(
            "t1".to_string(),
            vec![passing_assert("a"), failing_assert("b"), failing_assert("c")],
            std::time::Duration::ZERO,
        );
        assert_eq!(c.total_failed(), 2);
        assert_eq!(c.total_passed(), 1);
    }

    #[test]
    fn to_text_contains_results_summary() {
        let mut c = TestResultCollector::new();
        c.add(
            "t1".to_string(),
            vec![passing_assert("a"), failing_assert("b")],
            std::time::Duration::ZERO,
        );
        let text = c.to_text();
        assert!(text.contains("Results:"));
        assert!(text.contains("1 passed"));
        assert!(text.contains("1 failed"));
    }
}
