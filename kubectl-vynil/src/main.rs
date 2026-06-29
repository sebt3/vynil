use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;

use kubectl_vynil::{
    bundle::build_bundle,
    cli::{Cli, Commands, DiagnoseArgs, InstanceTarget},
    items::resolve_items,
    transport::{TransportMode, get_item, read_sa_token},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rustls::crypto::ring::default_provider().install_default().ok();

    let cli = Cli::parse();

    match cli.command {
        Commands::Diagnose(args) => run_diagnose(args).await,
    }
}

async fn run_diagnose(args: DiagnoseArgs) -> anyhow::Result<()> {
    let target =
        InstanceTarget::parse(&args.target, &args.namespace).map_err(|e| anyhow::anyhow!("{}", e))?;

    let items = resolve_items(args.items.as_deref());

    // Determine transport mode
    let (transport_mode, transport_label) = match &args.server_url {
        Some(url) => {
            let token = match &args.token {
                Some(t) => t.clone(),
                None => read_sa_token().context("no --token provided and cannot read in-cluster SA token")?,
            };
            (
                TransportMode::Direct {
                    server_url: url.clone(),
                    token,
                    insecure: args.insecure,
                },
                "direct",
            )
        }
        None => (TransportMode::Aggregation, "aggregation"),
    };

    // Collect items
    let mut collected: Vec<(&str, kubectl_vynil::transport::GetResult)> = Vec::new();
    for item in &items {
        eprintln!("collecting: {}", item);
        let result = get_item(&transport_mode, &target, item).await;
        collected.push((*item, result));
    }

    // Determine output path
    let output = args.output.unwrap_or_else(|| {
        let ts = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        format!("{}-diag-{}.tar.gz", target.name, ts)
    });
    let output_path = PathBuf::from(&output);

    // Build bundle
    eprintln!("building bundle: {}", output_path.display());
    let summary = build_bundle(&target, transport_label, collected, &output_path).await?;

    // Print summary
    println!("Bundle written to: {}", summary.output_path.display());
    println!("Items collected: {}", summary.item_count);
    println!(
        "Redactions: {} distinct values, {} occurrences",
        summary.total_redactions_distinct, summary.total_redactions_occurrences
    );
    if !summary.error_items.is_empty() {
        println!("Items with errors: {}", summary.error_items.join(", "));
    }

    Ok(())
}
