use clap::{Args, ValueEnum};
use common::{Result, vynilpackage::{VynilPackageType, read_package_yaml}};
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
        VynilPackageType::System => &["systems", "crds"],
        VynilPackageType::Service => &["vitals", "scalables", "others", "befores", "posts", "pods", "handlebars", "scripts"],
        VynilPackageType::Tenant => &["vitals", "scalables", "others", "befores", "posts", "pods", "handlebars", "scripts"],
    }
}

pub async fn run(args: &Parameters) -> Result<()> {
    let mut collector = crate::linting::LintResultCollector::new();
    let config = crate::linting::LintConfig::load(&args.package_dir)?;

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

    let package = match read_package_yaml(&manifest_path) {
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

    // Check 2: Invalid manifest (missing required fields)
    if package.metadata.name.is_empty() {
        collector.add(crate::linting::LintFinding {
            rule: "package/invalid-manifest".to_string(),
            level: crate::linting::LintLevel::Error,
            file: PathBuf::from("package.yaml"),
            line: None,
            col: None,
            message: "Missing required field: name".to_string(),
        });
    }
    if package.metadata.category.is_empty() {
        collector.add(crate::linting::LintFinding {
            rule: "package/invalid-manifest".to_string(),
            level: crate::linting::LintLevel::Error,
            file: PathBuf::from("package.yaml"),
            line: None,
            col: None,
            message: "Missing required field: category".to_string(),
        });
    }
    if package.metadata.description.is_empty() {
        collector.add(crate::linting::LintFinding {
            rule: "package/invalid-manifest".to_string(),
            level: crate::linting::LintLevel::Error,
            file: PathBuf::from("package.yaml"),
            line: None,
            col: None,
            message: "Missing required field: description".to_string(),
        });
    }

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

    // Check 4: Declared directories missing
    if let Some(images) = &package.images {
        if !images.is_empty() {
            let images_dir = args.package_dir.join("images");
            if !images_dir.exists() {
                collector.add(crate::linting::LintFinding {
                    rule: "package/declared-dir-missing".to_string(),
                    level: crate::linting::LintLevel::Warn,
                    file: PathBuf::from("package.yaml"),
                    line: None,
                    col: None,
                    message: "images section defined but images/ directory not found".to_string(),
                });
            }
        }
    }

    if let Some(resources) = &package.resources {
        if !resources.is_empty() {
            let resources_dir = args.package_dir.join("resources");
            if !resources_dir.exists() {
                collector.add(crate::linting::LintFinding {
                    rule: "package/declared-dir-missing".to_string(),
                    level: crate::linting::LintLevel::Warn,
                    file: PathBuf::from("package.yaml"),
                    line: None,
                    col: None,
                    message: "resources section defined but resources/ directory not found".to_string(),
                });
            }
        }
    }

    let level_filter = level_filter_to_lint_level(&args.level);
    println!("{}", collector.to_text(level_filter));

    if collector.has_errors() {
        Err(common::Error::YamlError("Linting failed with errors".to_string()))
    } else {
        Ok(())
    }
}

fn level_filter_to_lint_level(filter: &LevelFilter) -> crate::linting::LintLevel {
    match filter {
        LevelFilter::All => crate::linting::LintLevel::Info,
        LevelFilter::Warn => crate::linting::LintLevel::Warn,
        LevelFilter::Error => crate::linting::LintLevel::Error,
    }
}
