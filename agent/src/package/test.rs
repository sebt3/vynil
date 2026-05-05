use clap::{Args, ValueEnum};
use client::testing::TestHandler;
use common::{Error, Result, rhaihandler::Dynamic, yamlhandler::yaml_all_serialize_to_string};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};


#[derive(ValueEnum, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    #[value(name = "text")]
    Text,
    #[value(name = "json")]
    Json,
}

#[derive(clap::ValueEnum, Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PackageType {
    /// Tenant package type
    Tenant,
    /// Service package type
    Service,
    #[default]
    /// System package type
    System,
}

#[derive(Args, Debug, Serialize, Deserialize)]
pub struct Parameters {
    /// Package directory
    #[arg(
        short = 'p',
        long = "package-dir",
        env = "PACKAGE_DIRECTORY",
        value_name = "PACKAGE_DIRECTORY",
        default_value = "/tmp/package"
    )]
    package_dir: PathBuf,
    /// Agent script directory
    #[arg(
        short = 's',
        long = "script-dir",
        env = "SCRIPT_DIRECTORY",
        value_name = "SCRIPT_DIRECTORY",
        default_value = "./agent/scripts"
    )]
    script_dir: PathBuf,
    /// Agent template directory
    #[arg(
        short = 't',
        long = "template-dir",
        env = "TEMPLATE_DIRECTORY",
        value_name = "TEMPLATE_DIRECTORY",
        default_value = "./agent/templates"
    )]
    template_dir: PathBuf,
    /// Configuration directory
    #[arg(
        short = 'c',
        long = "config-dir",
        env = "CONFIG_DIR",
        value_name = "CONFIG_DIR",
        default_value = "."
    )]
    config_dir: PathBuf,
    /// testset additional directory (if any)
    #[arg(
        long = "testsets-dir",
        env = "TESTSETS_DIRECTORY",
        value_name = "TESTSETS_DIRECTORY"
    )]
    testset_dir: Option<PathBuf>,
    /// test-name
    #[arg(long = "test-name", env = "TEST_NAME", value_name = "TEST_NAME")]
    test_name: Option<String>,
    /// Start all tests
    #[arg(long = "all")]
    start_all: bool,
    /// Output format
    #[arg(long = "format", default_value = "text")]
    format: OutputFormat,
    /// junit output filename
    #[arg(
        long = "junit-output-filename",
        env = "JUNIT_OUTPUT_FILENAME",
        value_name = "JUNIT_OUTPUT_FILENAME"
    )]
    junit_output_filename: Option<PathBuf>,
    /// Template output filename (only available for a single test)
    #[arg(
        long = "template-output-filename",
        env = "TEMPLATE_OUTPUT_FILENAME",
        value_name = "TEMPLATE_OUTPUT_FILENAME"
    )]
    template_output_filename: Option<PathBuf>,
}

fn format_results(results: &client::testing::TestResultCollector, format: &OutputFormat) -> String {
    match format {
        OutputFormat::Json => results.to_json(),
        OutputFormat::Text => results.to_text(),
    }
}

pub async fn run(args: &Parameters) -> Result<()> {
    if !args.package_dir.join("tests").is_dir() {
        return Err(Error::MissingTestDirectory(args.package_dir.clone()));
    }
    let mut handler = TestHandler::new(
        args.package_dir.clone(),
        args.script_dir.clone(),
        args.config_dir.clone(),
        args.template_dir.clone(),
        args.testset_dir
            .clone()
            .map(|testset_dir| Vec::from(&[testset_dir])),
    )?;

    if args.start_all {
        handler.run_all_tests();
        println!("{}", format_results(&handler.results, &args.format));
        if let Some(output) = args.junit_output_filename.clone() {
            fs::write(output, handler.results.to_junit()).map_err(Error::Stdio)?;
        }
    } else if let Some(test_name) = args.test_name.clone() {
        let created_objects: Arc<Mutex<Vec<Dynamic>>> = Default::default();
        handler.run_test(&test_name, created_objects.clone());
        println!("{}", format_results(&handler.results, &args.format));
        if let Some(output) = args.junit_output_filename.clone() {
            fs::write(output, handler.results.to_junit()).map_err(Error::Stdio)?;
        }
        if let Some(output) = args.template_output_filename.clone() {
            let objets = created_objects.lock().unwrap().clone();
            let content = yaml_all_serialize_to_string(&objets)?;
            fs::write(output, content).map_err(Error::Stdio)?;
        }
    } else {
        for t in handler.list_tests() {
            println!("{t}");
        }
    }


    Ok(())
}
