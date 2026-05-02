use clap::{Args, ValueEnum};
use common::Result;
use std::path::PathBuf;

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
    package_dir: PathBuf,

    /// Configuration directory
    #[arg(
        short = 'c',
        long = "config-dir",
        env = "CONFIG_DIR",
        default_value = "."
    )]
    config_dir: PathBuf,

    /// Output format
    #[arg(long = "format", default_value = "text")]
    format: OutputFormat,

    /// Minimum level to display
    #[arg(long = "level", default_value = "all")]
    level: LevelFilter,

    /// JUnit output file
    #[arg(long = "junit-output-filename", env = "JUNIT_OUTPUT_FILENAME")]
    junit_output_filename: Option<PathBuf>,
}

pub async fn run(args: &Parameters) -> Result<()> {
    let collector = crate::linting::LintResultCollector::new();

    let level_filter = match args.level {
        LevelFilter::All => crate::linting::LintLevel::Info,
        LevelFilter::Warn => crate::linting::LintLevel::Warn,
        LevelFilter::Error => crate::linting::LintLevel::Error,
    };

    println!("{}", collector.to_text(level_filter));

    Ok(())
}
