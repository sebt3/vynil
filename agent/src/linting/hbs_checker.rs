use crate::linting::{LintFinding, LintLevel, LintConfig, parse_inline_disables};
use common::vynilpackage::VynilPackageSource;
use handlebars::template::{Template, TemplateElement, Parameter};
use handlebars::{Path as HbsPath, PathSeg};
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
    used_values: HashSet<String>,
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
            used_values: HashSet::new(),
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
                let mut walker = HelperWalker::new(file, self.config.clone(), inline_disables, self._pkg);
                walker.walk(&template);
                let walker_findings = walker.get_findings();
                let walker_used_helpers = walker.used_helpers;
                let walker_used_partials = walker.used_partials;
                let walker_used_values = walker.used_values;

                findings.extend(walker_findings);

                // Accumulate for unused checks
                self.used_helpers.extend(walker_used_helpers);
                self.used_partials.extend(walker_used_partials);
                self.used_values.extend(walker_used_values);
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

        // Check unused options
        if let Some(options) = &self._pkg.options {
            for key in options.keys() {
                if !self.used_values.contains(key) {
                    if let Some(level) = self.config.resolve_level(
                        "hbs/unused-option",
                        &PathBuf::from("package.yaml"),
                        LintLevel::Warn,
                        &HashSet::new(),
                    ) {
                        findings.push(LintFinding {
                            rule: "hbs/unused-option".to_string(),
                            level,
                            file: PathBuf::from("package.yaml"),
                            line: None,
                            col: None,
                            message: format!("Option `{}` defined but never used", key),
                        });
                    }
                }
            }
        }

        findings
    }
}

struct HelperWalker<'a> {
    file: PathBuf,
    config: LintConfig,
    inline_disables: HashMap<usize, HashSet<String>>,
    findings: Vec<LintFinding>,
    defined_helpers: HashSet<String>,
    defined_partials: HashSet<String>,
    pub used_helpers: HashSet<String>,
    pub used_partials: HashSet<String>,
    pub used_values: HashSet<String>,
    pkg: &'a VynilPackageSource,
}

impl<'a> HelperWalker<'a> {
    fn new(file: &Path, config: LintConfig, inline_disables: HashMap<usize, HashSet<String>>, pkg: &'a VynilPackageSource) -> Self {
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
            used_values: HashSet::new(),
            pkg,
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
                self.check_image_resource_helper(&h.name, &h.params);
                self.check_path(&h.name);
            }
            TemplateElement::Expression(e) => {
                // Check for helper references - Expressions with Name parameters are helpers
                self.check_helper(&e.name, &e.params);
                self.check_values_path(&e.name);
                self.check_image_resource_helper(&e.name, &e.params);
                self.check_path(&e.name);
            }
            TemplateElement::HtmlExpression(e) => {
                self.check_helper(&e.name, &e.params);
                self.check_values_path(&e.name);
                self.check_image_resource_helper(&e.name, &e.params);
                self.check_path(&e.name);
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

    fn check_values_path(&mut self, param: &Parameter) {
        if let Parameter::Path(path) = param {
            if let HbsPath::Relative((segs, _)) = path {
                if !segs.is_empty() {
                    if let PathSeg::Named(first) = &segs[0] {
                        if first == "values" && segs.len() >= 2 {
                            if let PathSeg::Named(key) = &segs[1] {
                                self.used_values.insert(key.clone());

                                if let Some(options) = &self.pkg.options {
                                    if !options.contains_key(key) {
                                        if let Some(level) = self.config.resolve_level(
                                            "hbs/unknown-value",
                                            &self.file,
                                            LintLevel::Error,
                                            self.inline_disables.get(&0).unwrap_or(&HashSet::new()),
                                        ) {
                                            self.findings.push(LintFinding {
                                                rule: "hbs/unknown-value".to_string(),
                                                level,
                                                file: self.file.clone(),
                                                line: None,
                                                col: None,
                                                message: format!("Unknown value key `{}`", key),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn check_image_resource_helper(&mut self, name: &Parameter, params: &[Parameter]) {
        if let Parameter::Name(helper_name) = name {
            let rule = if helper_name == "image_from_ctx" {
                Some(("hbs/unknown-image", "images"))
            } else if helper_name == "resources_from_ctx" {
                Some(("hbs/unknown-resource", "resources"))
            } else {
                None
            };

            if let Some((rule_name, field_name)) = rule {
                if params.len() > 1 {
                    if let Parameter::Literal(json_val) = &params[1] {
                        if let Some(key) = json_val.as_str() {
                            let has_key = if field_name == "images" {
                                self.pkg.images.as_ref().map_or(false, |imgs| imgs.contains_key(key))
                            } else {
                                self.pkg.resources.as_ref().map_or(false, |res| res.contains_key(key))
                            };

                            if !has_key && (
                                (field_name == "images" && self.pkg.images.is_some()) ||
                                (field_name == "resources" && self.pkg.resources.is_some())
                            ) {
                                if let Some(level) = self.config.resolve_level(
                                    rule_name,
                                    &self.file,
                                    LintLevel::Error,
                                    self.inline_disables.get(&0).unwrap_or(&HashSet::new()),
                                ) {
                                    self.findings.push(LintFinding {
                                        rule: rule_name.to_string(),
                                        level,
                                        file: self.file.clone(),
                                        line: None,
                                        col: None,
                                        message: format!("Unknown {} key `{}`", field_name.trim_end_matches('s'), key),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn check_path(&mut self, param: &Parameter) {
        if let Parameter::Path(path) = param {
            if let HbsPath::Relative((segs, _)) = path {
                if !segs.is_empty() {
                    if let PathSeg::Named(first) = &segs[0] {
                        if first == "tenant" && self.pkg.metadata.usage == common::vynilpackage::VynilPackageType::System {
                            if let Some(level) = self.config.resolve_level(
                                "hbs/wrong-package-type",
                                &self.file,
                                LintLevel::Warn,
                                self.inline_disables.get(&0).unwrap_or(&HashSet::new()),
                            ) {
                                self.findings.push(LintFinding {
                                    rule: "hbs/wrong-package-type".to_string(),
                                    level,
                                    file: self.file.clone(),
                                    line: None,
                                    col: None,
                                    message: "Accessing `tenant` in a System package is not allowed".to_string(),
                                });
                            }
                        }
                    }
                }
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
                used_values: HashSet::new(),
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

    #[test]
    fn known_value_key_no_finding() {
        let pkg = Box::leak(Box::new(create_pkg_with_options()));
        let config = Box::leak(Box::new(LintConfig::default()));
        let mut checker = HbsChecker {
            _package_dir: Path::new("."),
            _pkg: pkg,
            config,
            defined_helpers: HashSet::new(),
            used_helpers: HashSet::new(),
            defined_partials: HashSet::new(),
            used_partials: HashSet::new(),
            used_values: HashSet::new(),
        };

        let source = "{{values.port}}";
        let findings = checker.check_file(Path::new("test.hbs"), source);

        assert!(!findings.iter().any(|f| f.rule == "hbs/unknown-value"));
    }

    #[test]
    fn unknown_value_key_produces_error() {
        let pkg = Box::leak(Box::new(create_pkg_with_options()));
        let config = Box::leak(Box::new(LintConfig::default()));
        let mut checker = HbsChecker {
            _package_dir: Path::new("."),
            _pkg: pkg,
            config,
            defined_helpers: HashSet::new(),
            used_helpers: HashSet::new(),
            defined_partials: HashSet::new(),
            used_partials: HashSet::new(),
            used_values: HashSet::new(),
        };

        let source = "{{values.unknown_key}}";
        let findings = checker.check_file(Path::new("test.hbs"), source);

        assert!(findings.iter().any(|f| f.rule == "hbs/unknown-value"));
    }

    #[test]
    fn values_check_skipped_when_no_options() {
        let pkg = Box::leak(Box::new(create_dummy_pkg()));
        let config = Box::leak(Box::new(LintConfig::default()));
        let mut checker = HbsChecker {
            _package_dir: Path::new("."),
            _pkg: pkg,
            config,
            defined_helpers: HashSet::new(),
            used_helpers: HashSet::new(),
            defined_partials: HashSet::new(),
            used_partials: HashSet::new(),
            used_values: HashSet::new(),
        };

        let source = "{{values.anything}}";
        let findings = checker.check_file(Path::new("test.hbs"), source);

        assert!(!findings.iter().any(|f| f.rule == "hbs/unknown-value"));
    }

    #[test]
    fn unknown_image_key_produces_error() {
        let pkg = Box::leak(Box::new(create_pkg_with_images()));
        let config = Box::leak(Box::new(LintConfig::default()));
        let mut checker = HbsChecker {
            _package_dir: Path::new("."),
            _pkg: pkg,
            config,
            defined_helpers: HashSet::new(),
            used_helpers: HashSet::new(),
            defined_partials: HashSet::new(),
            used_partials: HashSet::new(),
            used_values: HashSet::new(),
        };

        let source = "{{image_from_ctx this \"missing\"}}";
        let findings = checker.check_file(Path::new("test.hbs"), source);

        assert!(findings.iter().any(|f| f.rule == "hbs/unknown-image"));
    }

    #[test]
    fn unknown_resource_key_produces_error() {
        let pkg = Box::leak(Box::new(create_pkg_with_resources()));
        let config = Box::leak(Box::new(LintConfig::default()));
        let mut checker = HbsChecker {
            _package_dir: Path::new("."),
            _pkg: pkg,
            config,
            defined_helpers: HashSet::new(),
            used_helpers: HashSet::new(),
            defined_partials: HashSet::new(),
            used_partials: HashSet::new(),
            used_values: HashSet::new(),
        };

        let source = "{{resources_from_ctx this \"missing\"}}";
        let findings = checker.check_file(Path::new("test.hbs"), source);

        assert!(findings.iter().any(|f| f.rule == "hbs/unknown-resource"));
    }

    #[test]
    fn tenant_access_in_system_package_warns() {
        let pkg = Box::leak(Box::new(create_system_pkg()));
        let config = Box::leak(Box::new(LintConfig::default()));
        let mut checker = HbsChecker {
            _package_dir: Path::new("."),
            _pkg: pkg,
            config,
            defined_helpers: HashSet::new(),
            used_helpers: HashSet::new(),
            defined_partials: HashSet::new(),
            used_partials: HashSet::new(),
            used_values: HashSet::new(),
        };

        let source = "{{tenant.name}}";
        let findings = checker.check_file(Path::new("test.hbs"), source);

        assert!(findings.iter().any(|f| f.rule == "hbs/wrong-package-type"));
        assert!(findings.iter().any(|f| f.level == LintLevel::Warn));
    }

    #[test]
    fn unused_option_produces_warning() {
        let pkg = Box::leak(Box::new(create_pkg_with_options()));
        let config = Box::leak(Box::new(LintConfig::default()));
        let mut checker = HbsChecker {
            _package_dir: Path::new("."),
            _pkg: pkg,
            config,
            defined_helpers: HashSet::new(),
            used_helpers: HashSet::new(),
            defined_partials: HashSet::new(),
            used_partials: HashSet::new(),
            used_values: HashSet::new(),
        };

        // Only use "port", but package has "port" and "host"
        let source = "{{values.port}}";
        checker.check_file(Path::new("test.hbs"), source);
        let final_findings = checker.finalize();

        assert!(final_findings.iter().any(|f| f.rule == "hbs/unused-option" && f.message.contains("host")));
    }

    fn create_pkg_with_options() -> VynilPackageSource {
        let mut options = std::collections::BTreeMap::new();
        options.insert("port".to_string(), serde_json::json!(8080));
        options.insert("host".to_string(), serde_json::json!("localhost"));

        VynilPackageSource {
            apiVersion: "v1".to_string(),
            kind: "VynilPackage".to_string(),
            metadata: VynilPackageMeta {
                name: "test".to_string(),
                category: "system".to_string(),
                description: "test package".to_string(),
                app_version: None,
                usage: common::vynilpackage::VynilPackageType::Tenant,
                features: vec![],
                backup_affinity: None,
            },
            requirements: vec![],
            recommandations: None,
            options: Some(options),
            images: None,
            resources: None,
            value_script: None,
        }
    }

    fn create_pkg_with_images() -> VynilPackageSource {
        let mut images = std::collections::BTreeMap::new();
        images.insert("app".to_string(), common::vynilpackage::Image {
            registry: "docker.io".to_string(),
            repository: "myapp".to_string(),
            tag: Some("1.0".to_string()),
        });

        VynilPackageSource {
            apiVersion: "v1".to_string(),
            kind: "VynilPackage".to_string(),
            metadata: VynilPackageMeta {
                name: "test".to_string(),
                category: "system".to_string(),
                description: "test package".to_string(),
                app_version: None,
                usage: common::vynilpackage::VynilPackageType::Tenant,
                features: vec![],
                backup_affinity: None,
            },
            requirements: vec![],
            recommandations: None,
            options: None,
            images: Some(images),
            resources: None,
            value_script: None,
        }
    }

    fn create_pkg_with_resources() -> VynilPackageSource {
        let mut resources = std::collections::BTreeMap::new();
        resources.insert("app".to_string(), common::vynilpackage::Resource {
            requests: Some(common::vynilpackage::ResourceItem {
                cpu: Some("100m".to_string()),
                memory: Some("128Mi".to_string()),
                storage: None,
            }),
            limits: Some(common::vynilpackage::ResourceItem {
                cpu: Some("500m".to_string()),
                memory: Some("512Mi".to_string()),
                storage: None,
            }),
            scaler: None,
        });

        VynilPackageSource {
            apiVersion: "v1".to_string(),
            kind: "VynilPackage".to_string(),
            metadata: VynilPackageMeta {
                name: "test".to_string(),
                category: "system".to_string(),
                description: "test package".to_string(),
                app_version: None,
                usage: common::vynilpackage::VynilPackageType::Tenant,
                features: vec![],
                backup_affinity: None,
            },
            requirements: vec![],
            recommandations: None,
            options: None,
            images: None,
            resources: Some(resources),
            value_script: None,
        }
    }

    fn create_system_pkg() -> VynilPackageSource {
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
}
