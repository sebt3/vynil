use clap::Parser;

use kubectl_vynil::{
    actions::{run_instance, run_jukebox},
    cli::{Cli, Commands, SERVICE_INSTANCE, SYSTEM_INSTANCE, TENANT_INSTANCE},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rustls::crypto::ring::default_provider().install_default().ok();

    let mut argv = std::env::args();
    let _bin = argv.next();
    if argv.next().as_deref() == Some("__complete") {
        let words: Vec<String> = argv.collect();
        kubectl_vynil::completion::run(words).await;
        return Ok(());
    }

    let cli = Cli::parse();
    let context = cli.context.as_deref();
    let namespace = cli.namespace.as_deref();

    match &cli.command {
        Commands::Jukebox(args) => run_jukebox(args, context).await,
        Commands::Vti(args) => run_instance(&TENANT_INSTANCE, args, context, namespace).await,
        Commands::Vsvc(args) => run_instance(&SERVICE_INSTANCE, args, context, namespace).await,
        Commands::Vsi(args) => run_instance(&SYSTEM_INSTANCE, args, context, namespace).await,
        Commands::Completion(args) => {
            kubectl_vynil::completion::emit_script(args.shell);
            Ok(())
        }
    }
}
