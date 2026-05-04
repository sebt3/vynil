use clap::{Args, ValueEnum};
use common::{Result, vynilpackage::{VynilPackageSource, VynilPackageType, read_package_yaml}};
use std::path::PathBuf;
use std::collections::HashSet;

#[derive(ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    #[value(name = "text")]
    Text,
    #[value(name = "json")]
    Json,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum LevelFilter {
    #[value(name = "all")]
    All,
    #[value(name = "warn")]
    Warn,
    #[value(name = "error")]
    Error,
}

#[derive(Args, Debug)]
pub struct Parameters {
    /// Package directory
    #[arg(
        short = 'p',
        long = "package-dir",
        env = "PACKAGE_DIRECTORY",
        default_value = "/tmp/package"
    )]
    pub package_dir: PathBuf,

    /// Configuration directory
    #[arg(
        short = 'c',
        long = "config-dir",
        env = "CONFIG_DIR",
        default_value = "."
    )]
    pub config_dir: PathBuf,

    /// Agent script directory (used to resolve shared imports like lib/)
    #[arg(
        short = 's',
        long = "script-dir",
        env = "SCRIPT_DIRECTORY",
        value_name = "SCRIPT_DIRECTORY",
        default_value = "./agent/scripts"
    )]
    pub script_dir: Option<PathBuf>,

    /// Output format
    #[arg(long = "format", default_value = "text")]
    pub format: OutputFormat,

    /// Minimum level to display
    #[arg(long = "level", default_value = "all")]
    pub level: LevelFilter,

    /// JUnit output file
    #[arg(long = "junit-output-filename", env = "JUNIT_OUTPUT_FILENAME")]
    pub junit_output_filename: Option<PathBuf>,
}

fn expected_dirs(pkg_type: &VynilPackageType) -> &[&str] {
    match pkg_type {
        VynilPackageType::System => &["systems", "crds", "scripts"],
        VynilPackageType::Service => &["vitals", "scalables", "others", "befores", "posts", "handlebars", "scripts", "crds"],
        VynilPackageType::Tenant => &["vitals", "scalables", "others", "befores", "posts", "handlebars", "scripts"],
    }
}

pub async fn run(args: &Parameters) -> Result<()> {
    let mut collector = crate::linting::LintResultCollector::new();
    let _config = crate::linting::LintConfig::load(&args.package_dir)?;

    // Check 1: Missing manifest
    let manifest_path = args.package_dir.join("package.yaml");
    if !manifest_path.exists() {
        collector.add(crate::linting::LintFinding {
            rule: "package/missing-manifest".to_string(),
            level: crate::linting::LintLevel::Error,
            file: PathBuf::from("package.yaml"),
            line: None,
            col: None,
            message: "package.yaml is missing".to_string(),
        });
        let level_filter = level_filter_to_lint_level(&args.level);
        println!("{}", collector.to_text(level_filter));
        return Err(common::Error::YamlError("Missing package manifest".to_string()));
    }

    let mut package = match read_package_yaml(&manifest_path) {
        Ok(pkg) => pkg,
        Err(e) => {
            collector.add(crate::linting::LintFinding {
                rule: "package/invalid-manifest".to_string(),
                level: crate::linting::LintLevel::Error,
                file: PathBuf::from("package.yaml"),
                line: None,
                col: None,
                message: format!("Failed to parse package.yaml: {}", e),
            });
            let level_filter = level_filter_to_lint_level(&args.level);
            println!("{}", collector.to_text(level_filter));
            return Err(common::Error::YamlError("Invalid package manifest".to_string()));
        }
    };

    // Check 2: Invalid manifest (missing required fields + option schemas)
    check_manifest_fields(&mut package, &mut collector);

    if collector.has_errors() {
        let level_filter = level_filter_to_lint_level(&args.level);
        println!("{}", collector.to_text(level_filter));
        return Err(common::Error::YamlError("Invalid package manifest".to_string()));
    }

    let expected = expected_dirs(&package.metadata.usage);
    let expected_set: HashSet<&str> = expected.iter().copied().collect();

    // Check 3: Unexpected directories
    if let Ok(entries) = std::fs::read_dir(&args.package_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                    if !dir_name.starts_with('.') && !expected_set.contains(dir_name) {
                        // Check if it's a known directory for other types
                        let all_known = ["systems", "crds", "vitals", "scalables", "others", "befores", "posts", "pods", "handlebars", "scripts"];
                        if all_known.contains(&dir_name) {
                            collector.add(crate::linting::LintFinding {
                                rule: "package/unexpected-dir".to_string(),
                                level: crate::linting::LintLevel::Warn,
                                file: PathBuf::from(dir_name),
                                line: None,
                                col: None,
                                message: format!("Directory '{}' is not expected for {} packages", dir_name, format!("{:?}", package.metadata.usage)),
                            });
                        }
                    }
                }
            }
        }
    }

    // Check HBS files
    let config = crate::linting::LintConfig::load(&args.package_dir)?;
    let mut hbs_checker = crate::linting::hbs_checker::HbsChecker::new(
        &args.package_dir,
        &package,
        &config,
    );

    // Scan for .hbs files
    if let Ok(entries) = std::fs::read_dir(&args.package_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_hbs_files(&path, &args.package_dir, &mut hbs_checker, &mut collector, &config, &package)?;
            }
        }
    }

    collector.extend(hbs_checker.finalize());

    // Check Rhai files
    let mut rhai_checker = crate::linting::rhai_checker::RhaiChecker::new(
        &args.package_dir,
        &args.config_dir,
        args.script_dir.as_deref(),
        &package,
        &config,
    );

    if let Ok(entries) = std::fs::read_dir(&args.package_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_rhai_files(&path, &args.package_dir, &mut rhai_checker, &mut collector)?;
            }
        }
    }

    collector.extend(rhai_checker.finalize());

    collector.prefix_files(&args.package_dir);

    let level_filter = level_filter_to_lint_level(&args.level);
    let text_output = collector.to_text(level_filter);
    println!("{}", text_output);

    if let Some(junit_path) = &args.junit_output_filename {
        let junit_xml = collector.to_junit();
        std::fs::write(junit_path, junit_xml)
            .map_err(|e| common::Error::YamlError(format!("Failed to write JUnit output: {}", e)))?;
    }

    if collector.has_errors() {
        Err(common::Error::YamlError("Linting failed with errors".to_string()))
    } else {
        Ok(())
    }
}

fn check_manifest_fields(package: &mut VynilPackageSource, collector: &mut crate::linting::LintResultCollector) {
    let manifest = PathBuf::from("package.yaml");
    let empty_fields: Vec<&str> = [
        ("name", package.metadata.name.is_empty()),
        ("category", package.metadata.category.is_empty()),
        ("description", package.metadata.description.is_empty()),
        ("apiVersion", package.apiVersion.is_empty()),
        ("kind", package.kind.is_empty()),
    ]
    .into_iter()
    .filter_map(|(field, empty)| if empty { Some(field) } else { None })
    .collect();

    for field in empty_fields {
        collector.add(crate::linting::LintFinding {
            rule: "package/invalid-manifest".to_string(),
            level: crate::linting::LintLevel::Error,
            file: manifest.clone(),
            line: None,
            col: None,
            message: format!("Missing required field: {}", field),
        });
    }
    if let Err(e) = package.validate_options() {
        collector.add(crate::linting::LintFinding {
            rule: "package/invalid-option-schema".to_string(),
            level: crate::linting::LintLevel::Error,
            file: manifest,
            line: None,
            col: None,
            message: format!("Invalid option schema: {}", e),
        });
    }
}

fn level_filter_to_lint_level(filter: &LevelFilter) -> crate::linting::LintLevel {
    match filter {
        LevelFilter::All => crate::linting::LintLevel::Info,
        LevelFilter::Warn => crate::linting::LintLevel::Warn,
        LevelFilter::Error => crate::linting::LintLevel::Error,
    }
}

fn scan_rhai_files(
    dir: &std::path::Path,
    package_dir: &std::path::Path,
    rhai_checker: &mut crate::linting::rhai_checker::RhaiChecker,
    collector: &mut crate::linting::LintResultCollector,
) -> Result<()> {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_rhai_files(&path, package_dir, rhai_checker, collector)?;
            } else if path.extension().and_then(|e| e.to_str()) == Some("rhai") {
                if let Ok(source) = std::fs::read_to_string(&path) {
                    if let Ok(rel_path) = path.strip_prefix(package_dir) {
                        let findings = rhai_checker.check_file(rel_path, &source);
                        for finding in findings {
                            collector.add(finding);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn scan_hbs_files(
    dir: &std::path::Path,
    package_dir: &std::path::Path,
    hbs_checker: &mut crate::linting::hbs_checker::HbsChecker,
    collector: &mut crate::linting::LintResultCollector,
    config: &crate::linting::LintConfig,
    package: &common::vynilpackage::VynilPackageSource,
) -> Result<()> {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_hbs_files(&path, package_dir, hbs_checker, collector, config, package)?;
            } else if path.extension().and_then(|e| e.to_str()) == Some("hbs") {
                if let Ok(source) = std::fs::read_to_string(&path) {
                    if let Ok(rel_path) = path.strip_prefix(package_dir) {
                        let findings = hbs_checker.check_file(rel_path, &source);
                        for finding in findings {
                            collector.add(finding);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::vynilpackage::{VynilPackageMeta, VynilPackageSource, VynilPackageType};

    fn make_valid_package() -> VynilPackageSource {
        VynilPackageSource {
            apiVersion: "vynil.solidite.fr/v1".to_string(),
            kind: "Package".to_string(),
            metadata: VynilPackageMeta {
                name: "test".to_string(),
                category: "apps".to_string(),
                description: "A test package".to_string(),
                app_version: None,
                usage: VynilPackageType::Tenant,
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
    fn check_manifest_fields_valid_has_no_errors() {
        let mut package = make_valid_package();
        let mut collector = crate::linting::LintResultCollector::new();
        check_manifest_fields(&mut package, &mut collector);
        assert!(!collector.has_errors());
    }

    #[test]
    fn check_manifest_fields_empty_name_is_error() {
        let mut package = make_valid_package();
        package.metadata.name = String::new();
        let mut collector = crate::linting::LintResultCollector::new();
        check_manifest_fields(&mut package, &mut collector);
        assert!(collector.has_errors());
    }

    #[test]
    fn check_manifest_fields_empty_api_version_is_error() {
        let mut package = make_valid_package();
        package.apiVersion = String::new();
        let mut collector = crate::linting::LintResultCollector::new();
        check_manifest_fields(&mut package, &mut collector);
        assert!(collector.has_errors());
    }

    #[test]
    fn check_manifest_fields_empty_kind_is_error() {
        let mut package = make_valid_package();
        package.kind = String::new();
        let mut collector = crate::linting::LintResultCollector::new();
        check_manifest_fields(&mut package, &mut collector);
        assert!(collector.has_errors());
    }

    #[test]
    fn check_manifest_fields_invalid_option_schema_is_error() {
        let mut package = make_valid_package();
        let mut options = std::collections::BTreeMap::new();
        options.insert("bad".to_string(), serde_json::json!(42));
        package.options = Some(options);
        let mut collector = crate::linting::LintResultCollector::new();
        check_manifest_fields(&mut package, &mut collector);
        assert!(collector.has_errors());
    }
}
