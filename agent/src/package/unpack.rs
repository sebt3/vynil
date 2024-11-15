use clap::Args;
use common::{ocihandler::Registry, rhaihandler::base64_decode, Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[allow(non_snake_case)]
#[derive(Deserialize, Serialize, Clone, Debug)]
struct Layer {
    digest: String,
    mediaType: String,
    size: i64,
}

#[derive(Args, Debug)]
pub struct Parameters {
    /// Destination directory
    #[arg(
        short = 'd',
        long = "destination",
        env = "PACKAGE_DIRECTORY",
        value_name = "PACKAGE_DIRECTORY",
        default_value = "/tmp/package"
    )]
    destination: PathBuf,
    /// Registry
    #[arg(
        short = 'r',
        long = "registry",
        env = "REGISTRY",
        value_name = "REGISTRY",
        default_value = "oci.solidite.fr"
    )]
    registry: String,
    /// Image
    #[arg(short = 'i', long = "image", env = "IMAGE", value_name = "IMAGE")]
    image: String,
    /// Tag
    #[arg(
        short = 't',
        long = "tag",
        env = "TAG",
        value_name = "TAG",
        default_value = "1.0.0"
    )]
    tag: String,
    /// Username
    #[arg(
        short = 'u',
        long = "username",
        env = "USERNAME",
        value_name = "USERNAME",
        default_value = ""
    )]
    username: String,
    /// Password
    #[arg(
        short = 'p',
        long = "password",
        env = "PASSWORD",
        value_name = "PASSWORD",
        default_value = ""
    )]
    password: String,
    /// pull-secret mount-path
    #[arg(
        long = "pull-secret-path",
        env = "PULL_SECRET_PATH",
        value_name = "PULL_SECRET_PATH",
        default_value = ""
    )]
    pull_path: String,
}

pub async fn run(args: &Parameters) -> Result<()> {
    if !Path::new(&args.destination).is_dir() {
        tracing::error!("{:?} is not a directory", &args.destination);
        Err(Error::MissingDestination(args.destination.clone()))
    } else {
        let mut cli = if args.pull_path.is_empty() {
            Registry::new(
                args.registry.clone(),
                args.username.clone(),
                args.password.clone(),
            )
        } else {
            let pull_secret_string = std::fs::read_to_string(format!("{}/.dockerconfigjson", args.pull_path))
                .map_err(Error::Stdio)?;
            let pull_secret: serde_json::Value =
                serde_json::from_str(&pull_secret_string).map_err(Error::SerializationError)?;
            let hash = pull_secret["auths"][args.registry.clone()]["auth"].clone();
            let user_pass = base64_decode(hash.as_str().unwrap().to_string())?;
            let auth = user_pass.split(":").collect::<Vec<&str>>();
            Registry::new(args.registry.clone(), auth[0].to_string(), auth[1].to_string())
        };
        cli.pull_image(&args.destination, args.image.clone(), args.tag.clone())
    }
}
