use super::vyniltestset::VynilAssertResult;
use junit_report::{Duration, ReportBuilder, TestCase, TestSuiteBuilder};

#[derive(Clone, Debug)]
pub struct TestResult {
    pub test_name: String,
    pub asserts: Vec<VynilAssertResult>,
    pub duration: std::time::Duration,
}

#[derive(Clone, Debug)]
pub struct TestResultCollector {
    results: Vec<TestResult>,
}

impl TestResultCollector {
    pub fn new() -> Self {
        Self { results: Vec::new() }
    }

    pub fn add(&mut self, test_name: String, asserts: Vec<VynilAssertResult>, duration: std::time::Duration) {
        self.results.push(TestResult {
            test_name,
            asserts,
            duration,
        });
    }

    pub fn total_tests(&self) -> usize {
        self.results.len()
    }

    pub fn total_asserts(&self) -> usize {
        self.results.iter().map(|r| r.asserts.len()).sum()
    }

    pub fn total_passed(&self) -> usize {
        self.results
            .iter()
            .flat_map(|r| &r.asserts)
            .filter(|a| a.passed)
            .count()
    }

    pub fn total_failed(&self) -> usize {
        self.results
            .iter()
            .flat_map(|r| &r.asserts)
            .filter(|a| !a.passed)
            .count()
    }

    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.asserts.iter().all(|a| a.passed))
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for result in &self.results {
            out.push_str(&format!(
                "Test: {} ({:.3}s)\n",
                result.test_name,
                result.duration.as_secs_f64()
            ));
            for a in &result.asserts {
                let status = if a.passed { "PASS" } else { "FAIL" };
                match &a.description {
                    Some(desc) if !desc.is_empty() => {
                        out.push_str(&format!("  [{status}] {} ({desc}): {}\n", a.name, a.message));
                    }
                    _ => {
                        out.push_str(&format!("  [{status}] {}: {}\n", a.name, a.message));
                    }
                }
            }
            out.push('\n');
        }
        out.push_str(&format!(
            "Results: {} passed, {} failed, {} total\n",
            self.total_passed(),
            self.total_failed(),
            self.total_asserts()
        ));
        out
    }

    pub fn to_junit(&self) -> String {
        let mut report_builder = ReportBuilder::new();
        for result in &self.results {
            let mut suite = TestSuiteBuilder::new(&result.test_name);
            let nanos = result.duration.as_nanos();
            let assert_count = result.asserts.len().max(1) as u128;
            let per_assert_nanos = nanos / assert_count;
            let tc_dur = Duration::new(
                (per_assert_nanos / 1_000_000_000) as i64,
                (per_assert_nanos % 1_000_000_000) as i32,
            );
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
        report.write_xml(&mut buf).expect("failed to write JUnit XML");
        String::from_utf8(buf).expect("JUnit XML is not valid UTF-8")
    }
}

impl Default for TestResultCollector {
    fn default() -> Self {
        Self::new()
    }
}
