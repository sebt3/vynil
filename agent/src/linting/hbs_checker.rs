use crate::linting::{LintFinding, LintLevel, LintConfig, parse_inline_disables};
use common::vynilpackage::VynilPackageSource;
use handlebars::template::{Template, TemplateElement, Parameter};
use handlebars::TemplateError;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const NATIVE_HELPERS: &[&str] = &[
    "base64_encode",
    "base64_decode",
    "url_encode",
    "to_decimal",
    "header_basic",
    "argon_hash",
    "bcrypt_hash",
    "crc32_hash",
    "gen_password",
    "gen_password_alphanum",
    "concat",
    "selector_from_ctx",
    "labels_from_ctx",
    "ctx_have_crd",
    "have_system_service",
    "have_tenant_service",
    "image_from_ctx",
    "resources_from_ctx",
    "render_template",
    "render_file",
    // handlebars_misc_helpers
    "to_json",
    "json_to_str",
    // handlebars built-ins (no-op blocks)
    "if",
    "unless",
    "each",
    "with",
    "lookup",
    "log",
    "raw",
    "inline",
];

pub struct HbsChecker<'a> {
    _package_dir: &'a Path,
    _pkg: &'a VynilPackageSource,
    config: &'a LintConfig,
    defined_helpers: HashSet<String>,
    used_helpers: HashSet<String>,
    defined_partials: HashSet<String>,
    used_partials: HashSet<String>,
}

impl<'a> HbsChecker<'a> {
    pub fn new(
        package_dir: &'a Path,
        pkg: &'a VynilPackageSource,
        config: &'a LintConfig,
    ) -> Self {
        let mut checker = HbsChecker {
            _package_dir: package_dir,
            _pkg: pkg,
            config,
            defined_helpers: HashSet::new(),
            used_helpers: HashSet::new(),
            defined_partials: HashSet::new(),
            used_partials: HashSet::new(),
        };

        // Scan handlebars/helpers/ for defined helpers
        let helpers_dir = package_dir.join("handlebars/helpers");
        if helpers_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&helpers_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map_or(false, |ext| ext == "rhai") {
                        if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
                            checker.defined_helpers.insert(name.to_string());
                        }
                    }
                }
            }
        }

        // Scan handlebars/partials/ for defined partials
        let partials_dir = package_dir.join("handlebars/partials");
        if partials_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&partials_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map_or(false, |ext| ext == "hbs") {
                        if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
                            checker.defined_partials.insert(name.to_string());
                        }
                    }
                }
            }
        }

        checker
    }

    pub fn check_file(&mut self, file: &Path, source: &str) -> Vec<LintFinding> {
        let mut findings = Vec::new();
        let inline_disables = parse_inline_disables(source);

        // Check 1: hbs/syntax
        match Template::compile(source) {
            Ok(template) => {
                // Check 2-5: helpers and partials
                let mut walker = HelperWalker::new(file, self.config.clone(), inline_disables);
                walker.walk(&template);
                let walker_findings = walker.get_findings();
                let walker_used_helpers = walker.used_helpers;
                let walker_used_partials = walker.used_partials;

                findings.extend(walker_findings);

                // Accumulate for unused checks
                self.used_helpers.extend(walker_used_helpers);
                self.used_partials.extend(walker_used_partials);
            }
            Err(e) => {
                if let Some(level) = self
                    .config
                    .resolve_level("hbs/syntax", file, LintLevel::Error, &HashSet::new())
                {
                    findings.push(LintFinding {
                        rule: "hbs/syntax".to_string(),
                        level,
                        file: file.to_path_buf(),
                        line: None,
                        col: None,
                        message: format!("Syntax error: {}", e),
                    });
                }
            }
        }

        findings
    }

    pub fn finalize(&self) -> Vec<LintFinding> {
        let mut findings = Vec::new();

        // Check unused helpers
        for helper in &self.defined_helpers {
            if !self.used_helpers.contains(helper) {
                if let Some(level) = self.config.resolve_level(
                    "hbs/unused-helper",
                    &PathBuf::from(&format!("handlebars/helpers/{}.rhai", helper)),
                    LintLevel::Warn,
                    &HashSet::new(),
                ) {
                    findings.push(LintFinding {
                        rule: "hbs/unused-helper".to_string(),
                        level,
                        file: PathBuf::from(&format!("handlebars/helpers/{}.rhai", helper)),
                        line: None,
                        col: None,
                        message: format!("Helper `{}` defined but never used", helper),
                    });
                }
            }
        }

        // Check unused partials
        for partial in &self.defined_partials {
            if !self.used_partials.contains(partial) {
                if let Some(level) = self.config.resolve_level(
                    "hbs/unused-partial",
                    &PathBuf::from(&format!("handlebars/partials/{}.hbs", partial)),
                    LintLevel::Warn,
                    &HashSet::new(),
                ) {
                    findings.push(LintFinding {
                        rule: "hbs/unused-partial".to_string(),
                        level,
                        file: PathBuf::from(&format!("handlebars/partials/{}.hbs", partial)),
                        line: None,
                        col: None,
                        message: format!("Partial `{}` defined but never used", partial),
                    });
                }
            }
        }

        findings
    }
}

struct HelperWalker {
    file: PathBuf,
    config: LintConfig,
    inline_disables: HashMap<usize, HashSet<String>>,
    findings: Vec<LintFinding>,
    defined_helpers: HashSet<String>,
    defined_partials: HashSet<String>,
    pub used_helpers: HashSet<String>,
    pub used_partials: HashSet<String>,
}

impl HelperWalker {
    fn new(file: &Path, config: LintConfig, inline_disables: HashMap<usize, HashSet<String>>) -> Self {
        // Collect defined helpers and partials from the first call
        // Note: This is initialized per file, but we track globally in HbsChecker
        HelperWalker {
            file: file.to_path_buf(),
            config,
            inline_disables,
            findings: Vec::new(),
            defined_helpers: HashSet::new(),
            defined_partials: HashSet::new(),
            used_helpers: HashSet::new(),
            used_partials: HashSet::new(),
        }
    }

    fn walk(&mut self, template: &Template) {
        Self::walk_template(template, &mut |elem| {
            self.visit_element(elem);
        });
    }

    fn walk_template<F>(tpl: &Template, visitor: &mut F)
    where
        F: FnMut(&TemplateElement),
    {
        for elem in &tpl.elements {
            visitor(elem);
            match elem {
                TemplateElement::HelperBlock(h) => {
                    if let Some(t) = &h.template {
                        Self::walk_template(t, visitor);
                    }
                    if let Some(t) = &h.inverse {
                        Self::walk_template(t, visitor);
                    }
                }
                TemplateElement::DecoratorBlock(d) | TemplateElement::PartialBlock(d) => {
                    if let Some(t) = &d.template {
                        Self::walk_template(t, visitor);
                    }
                }
                _ => {}
            }
        }
    }

    fn visit_element(&mut self, elem: &TemplateElement) {
        match elem {
            TemplateElement::HelperBlock(h) => {
                self.check_helper(&h.name, &h.params);
            }
            TemplateElement::Expression(e) => {
                // Check for helper references - Expressions with Name parameters are helpers
                self.check_helper(&e.name, &e.params);
            }
            TemplateElement::HtmlExpression(e) => {
                self.check_helper(&e.name, &e.params);
            }
            TemplateElement::DecoratorBlock(d) => {
                self.check_helper(&d.name, &d.params);
            }
            TemplateElement::PartialExpression(d) => {
                self.check_partial(&d.name);
            }
            TemplateElement::PartialBlock(d) => {
                self.check_partial(&d.name);
            }
            _ => {}
        }
    }

    fn check_helper(&mut self, name: &Parameter, params: &[Parameter]) {
        if let Parameter::Name(helper_name) = name {
            if !NATIVE_HELPERS.contains(&helper_name.as_str()) {
                self.used_helpers.insert(helper_name.clone());

                if !self.defined_helpers.contains(helper_name) {
                    if let Some(level) = self.config.resolve_level(
                        "hbs/unknown-helper",
                        &self.file,
                        LintLevel::Error,
                        self.inline_disables.get(&0).unwrap_or(&HashSet::new()),
                    ) {
                        self.findings.push(LintFinding {
                            rule: "hbs/unknown-helper".to_string(),
                            level,
                            file: self.file.clone(),
                            line: None,
                            col: None,
                            message: format!("Helper `{}` not found", helper_name),
                        });
                    }
                }
            }
        }

        // Check for nested subexpressions
        for param in params {
            if let Parameter::Subexpression(sub) = param {
                // Recurse into subexpression by visiting its element
                self.visit_element(sub.as_element());
            }
        }
    }

    fn check_partial(&mut self, name: &Parameter) {
        let partial_name = match name {
            Parameter::Name(s) => s.clone(),
            Parameter::Literal(v) => {
                v.as_str().unwrap_or("").to_string()
            }
            _ => return,
        };

        self.used_partials.insert(partial_name.clone());

        if !self.defined_partials.contains(&partial_name) {
            if let Some(level) = self.config.resolve_level(
                "hbs/unknown-partial",
                &self.file,
                LintLevel::Error,
                self.inline_disables.get(&0).unwrap_or(&HashSet::new()),
            ) {
                self.findings.push(LintFinding {
                    rule: "hbs/unknown-partial".to_string(),
                    level,
                    file: self.file.clone(),
                    line: None,
                    col: None,
                    message: format!("Partial `{}` not found", partial_name),
                });
            }
        }
    }

    fn get_findings(&self) -> Vec<LintFinding> {
        self.findings.clone()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use common::vynilpackage::VynilPackageMeta;

    struct TestChecker {
        checker: HbsChecker<'static>,
    }

    impl TestChecker {
        fn new(defined_helpers: Vec<&str>, defined_partials: Vec<&str>) -> Self {
            let pkg = Box::leak(Box::new(create_dummy_pkg()));
            let config = Box::leak(Box::new(LintConfig::default()));
            let checker = HbsChecker {
                _package_dir: Path::new("."),
                _pkg: pkg,
                config,
                defined_helpers: defined_helpers.iter().map(|s| s.to_string()).collect(),
                used_helpers: HashSet::new(),
                defined_partials: defined_partials.iter().map(|s| s.to_string()).collect(),
                used_partials: HashSet::new(),
            };
            TestChecker { checker }
        }
    }

    fn create_dummy_pkg() -> VynilPackageSource {
        VynilPackageSource {
            apiVersion: "v1".to_string(),
            kind: "VynilPackage".to_string(),
            metadata: VynilPackageMeta {
                name: "test".to_string(),
                category: "system".to_string(),
                description: "test package".to_string(),
                app_version: None,
                usage: common::vynilpackage::VynilPackageType::System,
                features: vec![],
                backup_affinity: None,
            },
            requirements: vec![],
            recommandations: None,
            options: None,
            images: None,
            resources: None,
            value_script: None,
        }
    }

    #[test]
    fn hbs_syntax_error_detected() {
        let mut test = TestChecker::new(vec![], vec![]);
        let source = "{{#if foo}}oops"; // bloc non fermé
        let findings = test.checker.check_file(Path::new("test.hbs"), source);

        assert!(!findings.is_empty());
        assert!(findings.iter().any(|f| f.rule == "hbs/syntax"));
        assert!(findings.iter().any(|f| f.level == LintLevel::Error));
    }

    #[test]
    fn known_helper_no_finding() {
        let mut test = TestChecker::new(vec![], vec![]);
        let source = "{{base64_encode val}}";
        let findings = test.checker.check_file(Path::new("test.hbs"), source);

        assert!(!findings.iter().any(|f| f.rule == "hbs/unknown-helper"));
    }

    #[test]
    fn unknown_helper_produces_error() {
        let mut test = TestChecker::new(vec![], vec![]);
        let source = "{{unknown_helper_xyz val}}";
        let findings = test.checker.check_file(Path::new("test.hbs"), source);

        assert!(findings.iter().any(|f| f.rule == "hbs/unknown-helper"));
    }

    #[test]
    fn used_partial_no_warning() {
        let mut test = TestChecker::new(vec![], vec!["mypartial"]);
        let source = "{{> mypartial}}";
        test.checker.check_file(Path::new("test.hbs"), source);
        let final_findings = test.checker.finalize();

        assert!(!final_findings
            .iter()
            .any(|f| f.rule == "hbs/unused-partial" && f.message.contains("mypartial")));
    }

    #[test]
    fn unused_partial_produces_warning() {
        let mut test = TestChecker::new(vec![], vec!["unused_partial"]);
        let source = "{{foo}}";
        test.checker.check_file(Path::new("test.hbs"), source);
        let final_findings = test.checker.finalize();

        assert!(final_findings
            .iter()
            .any(|f| f.rule == "hbs/unused-partial" && f.message.contains("unused_partial")));
    }

    #[test]
    fn unknown_partial_produces_error() {
        let mut test = TestChecker::new(vec![], vec![]);
        let source = "{{> nonexistent_partial}}";
        let findings = test.checker.check_file(Path::new("test.hbs"), source);

        assert!(findings.iter().any(|f| f.rule == "hbs/unknown-partial"));
    }
}
