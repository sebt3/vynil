mod clone;
mod install;
mod template;
mod plan;
mod destroy;

use clap::{Parser, Subcommand};
use std::process;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Parameters {
    #[command(subcommand)]
    pub command: Commands
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Clone given git repo as a distribution source
    Clone(clone::Parameters),
    /// Template the application dist files to kustomize compatible files
    Template(template::Parameters),
    /// Plan the install
    Plan(plan::Parameters),
    /// Install given component
    Install(install::Parameters),
    /// Destroy given component
    Destroy(destroy::Parameters),
}


#[tokio::main]
async fn main() {
    // TODO: Support importing resources
    // TODO: Support auto-import of existing resources
    // existing objects : k get inst -n kydah-vynil vynil -o "jsonpath={.status.tfstate.resources[*].instances[*].attributes.ids}"|jq .

    // import kustomized ones :
    // terraform show -json tf.plan|jq '.planned_values.root_module.resources[].address' -r|grep kustomization_resource.main|while read res;do terraform import "$res" "$(echo $res|sed 's/.*\["//;s/".*//')";done

    // import customressources
    // terraform show -json tf.plan>/tmp/plan.json
    // jq '.planned_values.root_module.resources[].address' -r /tmp/plan.json |nl -v 0|grep kubernetes_manifest|while read id res;do vers=$(jq ".planned_values.root_module.resources[$id].values.manifest.apiVersion" -r /tmp/plan.json);kind=$(jq ".planned_values.root_module.resources[$id].values.manifest.kind" -r /tmp/plan.json);name=$(jq ".planned_values.root_module.resources[$id].values.manifest.metadata.name" -r /tmp/plan.json);terraform import "$res" "apiVersion=$vers,kind=$kind,name=$name,namespace=solidite-auth";done

    env_logger::init_from_env(env_logger::Env::default().filter_or("LOG_LEVEL", "info").write_style_or("LOG_STYLE", "auto"));
    let args = Parameters::parse();
    match &args.command {
        Commands::Clone(args)   => {match clone::run(args).await {
            Ok(d) => d, Err(e) => {
                log::error!("Clone failed with: {e:}");
                process::exit(1)
            }
        }}
        // install init:1
        Commands::Template(args) => {match template::run(args).await {
            Ok(d) => d, Err(e) => {
                log::error!("Template failed with: {e:}");
                process::exit(1)
            }
        }},
        // install init:2 (or plan container, with the same previous init stage)
        Commands::Plan(args) => {match plan::run(args).await {
            Ok(d) => d, Err(e) => {
                log::error!("Plan failed with: {e:}");
                process::exit(1)
            }
        }},
        // install container
        Commands::Install(args) => {match install::run(args).await {
            Ok(d) => d, Err(e) => {
                log::error!("Install failed with: {e:}");
                process::exit(1)
            }
        }},
        // destroy container
        Commands::Destroy(args) => {match destroy::run(args).await {
            Ok(d) => d, Err(e) => {
                log::error!("Destroy failed with: {e:}");
                process::exit(1)
            }
        }},
    }
}
