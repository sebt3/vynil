use crate::linting::{LintFinding, LintLevel, LintConfig};
use common::vynilpackage::VynilPackageSource;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use rhai::{Engine, ASTNode, Stmt, Expr};

const ENTRY_POINT_PATTERNS: &[&str] = &[
    "install.rhai",
    "delete.rhai",
    "reconfigure.rhai",
];

pub struct RhaiChecker<'a> {
    package_dir: &'a Path,
    pkg: &'a VynilPackageSource,
    config: &'a LintConfig,
    resolver_paths: Vec<PathBuf>,
    importable_scripts: HashSet<PathBuf>,
    imported_scripts: HashSet<String>,
    defined_functions: HashSet<String>,
    called_functions: HashSet<String>,
}

impl<'a> RhaiChecker<'a> {
    pub fn new(
        package_dir: &'a Path,
        config_dir: &'a Path,
        pkg: &'a VynilPackageSource,
        config: &'a LintConfig,
    ) -> Self {
        let mut resolver_paths = vec![package_dir.join("scripts")];
        resolver_paths.push(config_dir.to_path_buf());

        let mut importable_scripts = HashSet::new();
        let scripts_dir = package_dir.join("scripts");
        if scripts_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&scripts_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map_or(false, |ext| ext == "rhai") {
                        // Exclude entry points and context_*.rhai
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            let is_entry_point = ENTRY_POINT_PATTERNS.iter().any(|p| name == *p)
                                || name.starts_with("context_");
                            if !is_entry_point {
                                importable_scripts.insert(path);
                            }
                        }
                    }
                }
            }
        }

        RhaiChecker {
            package_dir,
            pkg,
            config,
            resolver_paths,
            importable_scripts,
            imported_scripts: HashSet::new(),
            defined_functions: HashSet::new(),
            called_functions: HashSet::new(),
        }
    }

    pub fn check_file(&mut self, file: &Path, source: &str) -> Vec<LintFinding> {
        let mut findings = Vec::new();

        // Check 1: rhai/syntax
        let engine = Engine::new();
        match engine.compile(source) {
            Ok(ast) => {
                // Check 2: rhai/unresolved-import and accumulate imported scripts
                let import_findings = self.check_imports(&ast, file);
                findings.extend(import_findings);

                // Check 3: rhai/dead-code
                let dead_code_findings = check_dead_code(&ast, file);
                findings.extend(dead_code_findings);

                // Check 4: rhai/unused-variable and rhai/shadowed-variable
                let var_findings = check_unused_variables(&ast, file);
                findings.extend(var_findings);

                // Check 5: rhai/unused-parameter
                let param_findings = check_unused_parameters(&ast, file);
                findings.extend(param_findings);

                // Check 6: accumulate function definitions and calls for later finalize()
                self.accumulate_functions(&ast);
            }
            Err(e) => {
                if let Some(level) = self.config.resolve_level(
                    "rhai/syntax",
                    file,
                    LintLevel::Error,
                    &HashSet::new(),
                ) {
                    let (line, col) = extract_position(&e);
                    findings.push(LintFinding {
                        rule: "rhai/syntax".to_string(),
                        level,
                        file: file.to_path_buf(),
                        line,
                        col,
                        message: format!("Syntax error: {}", e),
                    });
                }
            }
        }

        findings
    }

    fn check_imports(&mut self, ast: &rhai::AST, file: &Path) -> Vec<LintFinding> {
        let mut findings = Vec::new();

        ast.walk(&mut |nodes: &[ASTNode]| {
            for node in nodes {
                if let ASTNode::Stmt(stmt) = node {
                    if let Stmt::Import(import_data, _) = stmt {
                        let (path_expr, _alias) = &**import_data;
                        if let Expr::StringConstant(module_name, _) = path_expr {
                            self.imported_scripts.insert(module_name.to_string());
                            if !self.resolve_import(module_name) {
                                if let Some(level) = self.config.resolve_level(
                                    "rhai/unresolved-import",
                                    file,
                                    LintLevel::Error,
                                    &HashSet::new(),
                                ) {
                                    findings.push(LintFinding {
                                        rule: "rhai/unresolved-import".to_string(),
                                        level,
                                        file: file.to_path_buf(),
                                        line: None,
                                        col: None,
                                        message: format!(
                                            "Cannot resolve import \"{}\"",
                                            module_name
                                        ),
                                    });
                                }
                            }
                        }
                    }
                }
            }
            true
        });

        findings
    }

    fn resolve_import(&self, module_name: &str) -> bool {
        let module_file = format!("{}.rhai", module_name);
        for resolver_path in &self.resolver_paths {
            let full_path = resolver_path.join(&module_file);
            if full_path.exists() {
                return true;
            }
        }
        false
    }

    fn accumulate_functions(&mut self, ast: &rhai::AST) {
        // Collect defined functions
        for fn_def in ast.iter_fn_def() {
            self.defined_functions.insert(fn_def.name.to_string());
        }

        // Collect called functions
        ast.walk(&mut |nodes: &[ASTNode]| {
            for node in nodes {
                match node {
                    ASTNode::Expr(expr) => {
                        if let Expr::FnCall(fn_call, _) = expr {
                            self.called_functions.insert(fn_call.name.to_string());
                        } else if let Expr::MethodCall(fn_call, _) = expr {
                            self.called_functions.insert(fn_call.name.to_string());
                        }
                    }
                    _ => {}
                }
            }
            true
        });
    }

    pub fn finalize(&self) -> Vec<LintFinding> {
        let mut findings = Vec::new();

        // Check unused-script
        for script_path in &self.importable_scripts {
            if let Some(file_stem) = script_path.file_stem().and_then(|s| s.to_str()) {
                if !self.imported_scripts.contains(file_stem) {
                    if let Some(level) = self.config.resolve_level(
                        "rhai/unused-script",
                        script_path,
                        LintLevel::Warn,
                        &HashSet::new(),
                    ) {
                        findings.push(LintFinding {
                            rule: "rhai/unused-script".to_string(),
                            level,
                            file: script_path.to_path_buf(),
                            line: None,
                            col: None,
                            message: format!(
                                "Script `{}` defined but never imported",
                                file_stem
                            ),
                        });
                    }
                }
            }
        }

        // Check unused-function
        let entry_points = ["run", "template", "new", "main"];
        for func_name in &self.defined_functions {
            if !self.called_functions.contains(func_name)
                && !entry_points.contains(&func_name.as_str())
            {
                if let Some(level) = self.config.resolve_level(
                    "rhai/unused-function",
                    &PathBuf::from("scripts"),
                    LintLevel::Warn,
                    &HashSet::new(),
                ) {
                    findings.push(LintFinding {
                        rule: "rhai/unused-function".to_string(),
                        level,
                        file: PathBuf::from("scripts"),
                        line: None,
                        col: None,
                        message: format!(
                            "Function `{}` defined but never called",
                            func_name
                        ),
                    });
                }
            }
        }

        findings
    }
}

fn check_dead_code(ast: &rhai::AST, file: &Path) -> Vec<LintFinding> {
    let mut findings = Vec::new();

    // Check top-level statements
    check_statements_for_dead_code(ast.statements(), file, &mut findings);

    findings
}

fn check_statements_for_dead_code(
    statements: &[Stmt],
    file: &Path,
    findings: &mut Vec<LintFinding>,
) {
    for (idx, stmt) in statements.iter().enumerate() {
        // If this is a terminating statement and there are statements after it, report dead code
        if matches!(stmt, Stmt::Return(..) | Stmt::BreakLoop(..)) {
            for _dead_idx in (idx + 1)..statements.len() {
                findings.push(LintFinding {
                    rule: "rhai/dead-code".to_string(),
                    level: LintLevel::Warn,
                    file: file.to_path_buf(),
                    line: None,
                    col: None,
                    message: "Unreachable code after return or break".to_string(),
                });
            }
            break;
        }

        // Recursively check nested blocks
        match stmt {
            Stmt::Block(block) => {
                check_statements_for_dead_code(block.statements(), file, findings);
            }
            Stmt::If(flow_control, ..) => {
                check_statements_for_dead_code(flow_control.body.statements(), file, findings);
                check_statements_for_dead_code(flow_control.branch.statements(), file, findings);
            }
            Stmt::While(flow_control, ..) | Stmt::Do(flow_control, ..) => {
                check_statements_for_dead_code(flow_control.body.statements(), file, findings);
            }
            Stmt::For(data, ..) => {
                check_statements_for_dead_code(data.2.body.statements(), file, findings);
            }
            Stmt::TryCatch(flow_control, ..) => {
                check_statements_for_dead_code(flow_control.body.statements(), file, findings);
                check_statements_for_dead_code(flow_control.branch.statements(), file, findings);
            }
            _ => {}
        }
    }
}

fn extract_position(e: &rhai::ParseError) -> (Option<usize>, Option<usize>) {
    let pos = e.position();
    let line = pos.line();
    let col = pos.position();
    (line, col)
}

fn check_unused_variables(ast: &rhai::AST, file: &Path) -> Vec<LintFinding> {
    let mut findings = Vec::new();
    let mut declarations: Vec<(String, usize)> = Vec::new();
    let mut used: HashSet<String> = HashSet::new();

    // Pass 1: collect variable declarations
    ast.walk(&mut |nodes: &[ASTNode]| {
        for node in nodes {
            if let ASTNode::Stmt(stmt) = node {
                if let Stmt::Var(var_data, _, pos) = stmt {
                    let (ident, _, _) = &**var_data;
                    let name = ident.name.to_string();
                    declarations.push((name, pos.line().unwrap_or(0)));
                }
            }
        }
        true
    });

    // Pass 2: collect variable usages
    ast.walk(&mut |nodes: &[ASTNode]| {
        for node in nodes {
            if let ASTNode::Expr(expr) = node {
                if let Expr::Variable(var_data, _, _) = expr {
                    let (_, name, _, _) = &**var_data;
                    used.insert(name.to_string());
                }
            }
        }
        true
    });

    // Check for shadowing
    let mut seen: HashSet<String> = HashSet::new();
    for (name, line) in &declarations {
        if !name.starts_with('_') {
            if seen.contains(name) {
                findings.push(LintFinding {
                    rule: "rhai/shadowed-variable".to_string(),
                    level: LintLevel::Warn,
                    file: file.to_path_buf(),
                    line: Some(*line),
                    col: None,
                    message: format!("Variable `{}` shadows a previous declaration", name),
                });
            }
            seen.insert(name.clone());
        }
    }

    // Check for unused variables
    for (name, line) in declarations {
        if !name.starts_with('_') && !used.contains(&name) {
            findings.push(LintFinding {
                rule: "rhai/unused-variable".to_string(),
                level: LintLevel::Warn,
                file: file.to_path_buf(),
                line: Some(line),
                col: None,
                message: format!("Variable `{}` is declared but never used", name),
            });
        }
    }

    findings
}

fn check_unused_parameters(ast: &rhai::AST, file: &Path) -> Vec<LintFinding> {
    let mut findings = Vec::new();

    for fn_def in ast.iter_fn_def() {
        let mut used_params: HashSet<String> = HashSet::new();

        // Walk the entire AST looking for variable references within function bodies
        // We use a simple heuristic: collect all variable references and filter by known params
        ast.walk(&mut |nodes: &[ASTNode]| {
            for node in nodes {
                if let ASTNode::Expr(expr) = node {
                    if let Expr::Variable(var_data, _, _) = expr {
                        let (_, name, _, _) = &**var_data;
                        used_params.insert(name.to_string());
                    }
                }
            }
            true
        });

        // Check for unused parameters
        for param_name in &fn_def.params {
            let param_str = param_name.to_string();
            if !param_str.starts_with('_') && !used_params.contains(&param_str) {
                findings.push(LintFinding {
                    rule: "rhai/unused-parameter".to_string(),
                    level: LintLevel::Warn,
                    file: file.to_path_buf(),
                    line: None,
                    col: None,
                    message: format!(
                        "Parameter `{}` in function `{}` is never used",
                        param_str, fn_def.name
                    ),
                });
            }
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::vynilpackage::read_package_yaml;

    fn get_fixture_dir(name: &str) -> PathBuf {
        let base = env!("CARGO_MANIFEST_DIR");
        PathBuf::from(format!("{}/tests/fixtures/lint/{}/", base, name))
    }

    #[test]
    fn rhai_syntax_error_detected() {
        let base_dir = get_fixture_dir("rhai-checks");
        let pkg = read_package_yaml(&base_dir.join("package.yaml")).expect("Failed to load package");
        let config = LintConfig::default();
        let mut checker = RhaiChecker::new(&base_dir, &base_dir, &pkg, &config);

        let source = "let x = ;"; // syntax invalid
        let findings = checker.check_file(&PathBuf::from("test.rhai"), source);

        assert!(
            findings.iter().any(|f| f.rule == "rhai/syntax"),
            "Expected rhai/syntax finding"
        );
    }

    #[test]
    fn valid_rhai_no_syntax_error() {
        let base_dir = get_fixture_dir("rhai-checks");
        let pkg = read_package_yaml(&base_dir.join("package.yaml")).expect("Failed to load package");
        let config = LintConfig::default();
        let mut checker = RhaiChecker::new(&base_dir, &base_dir, &pkg, &config);

        let source = "let x = 1;";
        let findings = checker.check_file(&PathBuf::from("test.rhai"), source);

        assert!(
            !findings.iter().any(|f| f.rule == "rhai/syntax"),
            "Expected no rhai/syntax finding"
        );
    }

    #[test]
    fn resolved_import_no_error() {
        let base_dir = get_fixture_dir("rhai-checks");
        let pkg = read_package_yaml(&base_dir.join("package.yaml")).expect("Failed to load package");
        let config = LintConfig::default();
        let mut checker = RhaiChecker::new(&base_dir, &base_dir, &pkg, &config);

        let source = r#"import "mylib" as lib;"#;
        let findings = checker.check_file(&PathBuf::from("test.rhai"), source);

        assert!(
            !findings
                .iter()
                .any(|f| f.rule == "rhai/unresolved-import"),
            "Expected no rhai/unresolved-import finding"
        );
    }

    #[test]
    fn unresolved_import_produces_error() {
        let base_dir = get_fixture_dir("rhai-checks");
        let pkg = read_package_yaml(&base_dir.join("package.yaml")).expect("Failed to load package");
        let config = LintConfig::default();
        let mut checker = RhaiChecker::new(&base_dir, &base_dir, &pkg, &config);

        let source = r#"import "nonexistent" as x;"#;
        let findings = checker.check_file(&PathBuf::from("test.rhai"), source);

        assert!(
            findings
                .iter()
                .any(|f| f.rule == "rhai/unresolved-import"),
            "Expected rhai/unresolved-import finding"
        );
    }

    #[test]
    #[ignore = "dead code detection in function bodies requires private AST API access"]
    fn dead_code_after_return_detected() {
        let base_dir = get_fixture_dir("rhai-checks");
        let pkg = read_package_yaml(&base_dir.join("package.yaml")).expect("Failed to load package");
        let config = LintConfig::default();
        let mut checker = RhaiChecker::new(&base_dir, &base_dir, &pkg, &config);

        // TODO: Test function body dead code detection
        let source = "fn foo() { return 1; let x = 2; }";
        let findings = checker.check_file(&PathBuf::from("test.rhai"), source);

        assert!(
            findings.iter().any(|f| f.rule == "rhai/dead-code"),
            "Expected rhai/dead-code finding"
        );
    }

    #[test]
    fn unused_script_produces_warning() {
        let base_dir = get_fixture_dir("rhai-checks");
        let pkg = read_package_yaml(&base_dir.join("package.yaml")).expect("Failed to load package");
        let config = LintConfig::default();
        let mut checker = RhaiChecker::new(&base_dir, &base_dir, &pkg, &config);

        // Check install.rhai (which doesn't import orphan)
        let install_source = "fn run() {}";
        let _ = checker.check_file(&PathBuf::from("scripts/install.rhai"), install_source);

        let findings = checker.finalize();

        assert!(
            findings
                .iter()
                .any(|f| f.rule == "rhai/unused-script" && f.message.contains("orphan")),
            "Expected rhai/unused-script finding for orphan"
        );
    }

    #[test]
    fn used_variable_no_warning() {
        let base_dir = get_fixture_dir("rhai-checks");
        let pkg = read_package_yaml(&base_dir.join("package.yaml")).expect("Failed to load package");
        let config = LintConfig::default();
        let mut checker = RhaiChecker::new(&base_dir, &base_dir, &pkg, &config);

        let source = "let x = 1; x + 1;";
        let findings = checker.check_file(&PathBuf::from("test.rhai"), source);

        assert!(
            !findings.iter().any(|f| f.rule == "rhai/unused-variable"),
            "Expected no rhai/unused-variable finding"
        );
    }

    #[test]
    fn unused_variable_produces_warning() {
        let base_dir = get_fixture_dir("rhai-checks");
        let pkg = read_package_yaml(&base_dir.join("package.yaml")).expect("Failed to load package");
        let config = LintConfig::default();
        let mut checker = RhaiChecker::new(&base_dir, &base_dir, &pkg, &config);

        let source = "let unused = 42;";
        let findings = checker.check_file(&PathBuf::from("test.rhai"), source);

        assert!(
            findings
                .iter()
                .any(|f| f.rule == "rhai/unused-variable" && f.message.contains("unused")),
            "Expected rhai/unused-variable finding for 'unused'"
        );
    }

    #[test]
    fn underscore_variable_ignored() {
        let base_dir = get_fixture_dir("rhai-checks");
        let pkg = read_package_yaml(&base_dir.join("package.yaml")).expect("Failed to load package");
        let config = LintConfig::default();
        let mut checker = RhaiChecker::new(&base_dir, &base_dir, &pkg, &config);

        let source = "let _unused = 42;";
        let findings = checker.check_file(&PathBuf::from("test.rhai"), source);

        assert!(
            !findings.iter().any(|f| f.rule == "rhai/unused-variable"),
            "Expected no rhai/unused-variable finding for underscore-prefixed variables"
        );
    }

    #[test]
    fn shadowed_variable_produces_warning() {
        let base_dir = get_fixture_dir("rhai-checks");
        let pkg = read_package_yaml(&base_dir.join("package.yaml")).expect("Failed to load package");
        let config = LintConfig::default();
        let mut checker = RhaiChecker::new(&base_dir, &base_dir, &pkg, &config);

        let source = "let x = 1; let x = 2;";
        let findings = checker.check_file(&PathBuf::from("test.rhai"), source);

        assert!(
            findings
                .iter()
                .any(|f| f.rule == "rhai/shadowed-variable"),
            "Expected rhai/shadowed-variable finding"
        );
    }

    #[test]
    fn unused_parameter_produces_warning() {
        let base_dir = get_fixture_dir("rhai-checks");
        let pkg = read_package_yaml(&base_dir.join("package.yaml")).expect("Failed to load package");
        let config = LintConfig::default();
        let mut checker = RhaiChecker::new(&base_dir, &base_dir, &pkg, &config);

        let source = "fn foo(used, unused) { used }";
        let findings = checker.check_file(&PathBuf::from("test.rhai"), source);

        assert!(
            findings
                .iter()
                .any(|f| f.rule == "rhai/unused-parameter" && f.message.contains("unused")),
            "Expected rhai/unused-parameter finding for 'unused'"
        );
    }

    #[test]
    fn unused_function_produces_warning() {
        let base_dir = get_fixture_dir("rhai-checks");
        let pkg = read_package_yaml(&base_dir.join("package.yaml")).expect("Failed to load package");
        let config = LintConfig::default();
        let mut checker = RhaiChecker::new(&base_dir, &base_dir, &pkg, &config);

        let source = "fn helper() { 42 }";
        let _ = checker.check_file(&PathBuf::from("test.rhai"), source);

        let findings = checker.finalize();

        assert!(
            findings
                .iter()
                .any(|f| f.rule == "rhai/unused-function" && f.message.contains("helper")),
            "Expected rhai/unused-function finding for 'helper'"
        );
    }

    #[test]
    fn run_function_not_flagged_as_unused() {
        let base_dir = get_fixture_dir("rhai-checks");
        let pkg = read_package_yaml(&base_dir.join("package.yaml")).expect("Failed to load package");
        let config = LintConfig::default();
        let mut checker = RhaiChecker::new(&base_dir, &base_dir, &pkg, &config);

        let source = "fn run(inst, ctx, args) { 42 }";
        let _ = checker.check_file(&PathBuf::from("test.rhai"), source);

        let findings = checker.finalize();

        assert!(
            !findings
                .iter()
                .any(|f| f.rule == "rhai/unused-function" && f.message.contains("run")),
            "Expected no rhai/unused-function finding for 'run' entry point"
        );
    }
}
