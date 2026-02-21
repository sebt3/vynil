use crate::{
    Error, Result, RhaiRes, Semver, instanceservice::ServiceInstance, instancesystem::SystemInstance,
    instancetenant::TenantInstance, rhai_err, rhaihandler::Script,
};
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::{Api, Client, api::ListParams};
pub use openapiv3::Schema;
use rhai::{Dynamic, Engine};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn get_vynil_version() -> String {
    VERSION.to_string()
}

/// Vynil package type
#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum VynilPackageType {
    #[default]
    Tenant,
    System,
    Service,
}

/// Vynil package feature
#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum VynilPackageFeature {
    #[default]
    Upgrade,
    Backup,
    Monitoring,
    HighAvailability,
    AutoConfig,
    AutoScaling,
    Deprecated,
}

/// Vynil Package Meta
#[derive(Deserialize, Serialize, Clone, PartialEq, Debug, JsonSchema)]
pub struct VynilPackageMeta {
    /// Package name
    pub name: String,
    /// Package category
    pub category: String,
    /// Package description
    pub description: String,
    /// Application version
    pub app_version: Option<String>,
    /// Package type
    #[serde(rename = "type")]
    pub usage: VynilPackageType,
    /// Package features
    pub features: Vec<VynilPackageFeature>,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, JsonSchema, Default)]
pub enum StorageCapability {
    #[default]
    RWX,
    ROX,
}

/// Vynil Package Requirement
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VynilPackageRequirement {
    /// Name of a crd that is required before installing this package
    CustomResourceDefinition(String),
    /// Name of a System Service that should be installed before current package
    SystemService(String),
    /// Name of a Tenant Service that should be installed before current package
    TenantService(String),
    /// SystemPackage that should be installed before current package
    SystemPackage {
        category: String,
        name: String,
    },
    /// TenantPackage that should be installed before current package in the current Tenant
    TenantPackage {
        category: String,
        name: String,
    },
    /// a rhai script that return a boolean
    Prefly {
        script: String,
        name: String,
    },
    // Check minimum cluster version
    ClusterVersion {
        major: u64,
        minor: u64,
    },
    StorageCapability(StorageCapability),
    /// Forbid migration that are not supported
    MinimumPreviousVersion(String),
    /// Minimum vynil version
    VynilVersion(String),
    /// Sum of all requests (Informative only)
    Cpu(f64),
    // MB, Sum of all requests (Informative only)
    Memory(u64),
    // MB, Sum of all requests (Informative only)
    Disk(u64),
}
impl VynilPackageRequirement {
    pub async fn check_system(&self, inst: &SystemInstance, client: Client) -> Result<(bool, String, u64)> {
        match self {
            VynilPackageRequirement::VynilVersion(v) => {
                let requested = Semver::parse(v)?;
                let current = Semver::parse(VERSION)?;
                Ok((
                    current >= requested,
                    format!(
                        "Requested vynil version {v} is over current version {VERSION}. Please upgrade vynil first"
                    ),
                    15 * 60,
                ))
            }
            VynilPackageRequirement::ClusterVersion { major, minor } => {
                let raw = crate::k8sraw::K8sRaw::new();
                let ver = raw.get_api_version().await?;
                let maj: u64 = serde_json::to_string(&ver.as_object().unwrap()["major"])
                    .map_err(Error::SerializationError)?
                    .parse()
                    .map_err(Error::ParseInt)?;
                let min: u64 = serde_json::to_string(&ver.as_object().unwrap()["minor"])
                    .map_err(Error::SerializationError)?
                    .parse()
                    .map_err(Error::ParseInt)?;
                Ok((
                    maj > *major || (maj == *major && min >= *minor),
                    format!(
                        "Requested api-server version {major}.{minor} is over current version {maj}.{min}. Please upgrade your cluster first"
                    ),
                    15 * 60,
                ))
            }
            VynilPackageRequirement::CustomResourceDefinition(crd) => {
                let api: Api<CustomResourceDefinition> = Api::all(client);
                let r = api.get_metadata_opt(crd).await.map_err(Error::KubeError)?;
                Ok((r.is_some(), format!("CRD {crd} is not installed"), 5 * 60))
            }
            VynilPackageRequirement::Prefly { script, name } => {
                let mut rhai = Script::new(vec![]);
                rhai.ctx.set_value("instance", inst.clone());
                Ok((
                    rhai.eval_truth(script)?,
                    format!("Requirement {name} failed"),
                    5 * 60,
                ))
            }
            VynilPackageRequirement::SystemPackage { category, name } => {
                let api: Api<SystemInstance> = Api::all(client);
                let lst = api.list(&ListParams::default()).await.map_err(Error::KubeError)?;
                Ok((
                    lst.items
                        .into_iter()
                        .any(|i| i.spec.category == *category && i.spec.package == *name),
                    format!("System package {category}/{name} is not installed"),
                    15 * 60,
                ))
            }
            VynilPackageRequirement::TenantPackage { category, name } => {
                tracing::warn!("TenantPackage Requirement for a system package is invalid, skipping");
                Ok((
                    true,
                    format!("Tenant package {category}/{name} is not installed"),
                    15 * 60,
                ))
            }
            VynilPackageRequirement::StorageCapability(capa) => {
                //TODO: implement StorageCapability
                tracing::warn!("StorageCapability Requirement is a TODO");
                Ok((
                    true,
                    format!("Storage capability {:?} isn't available", capa),
                    15 * 60,
                ))
            }
            VynilPackageRequirement::MinimumPreviousVersion(prev) => {
                //TODO: implement MinimumPreviousVersion
                tracing::warn!("MinimumPreviousVersion Requirement is a TODO");
                Ok((true, format!("Minimum {prev} version is not available"), 15 * 60))
            }
            VynilPackageRequirement::SystemService(svc) => {
                let lst = ServiceInstance::get_all_services_names().await?;
                Ok((
                    lst.iter().any(|i| svc == i),
                    format!("System Service {svc} is not available"),
                    15 * 60,
                ))
            }
            _ => Ok((true, "".to_string(), 15 * 60)),
        }
    }

    pub async fn check_tenant(&self, inst: &TenantInstance, client: Client) -> Result<(bool, String, u64)> {
        match self {
            VynilPackageRequirement::VynilVersion(v) => {
                let requested = Semver::parse(v)?;
                let current = Semver::parse(VERSION)?;
                Ok((
                    current >= requested,
                    format!(
                        "Requested vynil version {v} is over current version {VERSION}. Please upgrade vynil first"
                    ),
                    15 * 60,
                ))
            }
            VynilPackageRequirement::ClusterVersion { major, minor } => {
                let raw = crate::k8sraw::K8sRaw::new();
                let ver = raw.get_api_version().await?;
                let maj: u64 = serde_json::to_string(&ver.as_object().unwrap()["major"])
                    .map_err(Error::SerializationError)?
                    .parse()
                    .map_err(Error::ParseInt)?;
                let min: u64 = serde_json::to_string(&ver.as_object().unwrap()["minor"])
                    .map_err(Error::SerializationError)?
                    .parse()
                    .map_err(Error::ParseInt)?;
                Ok((
                    maj > *major || (maj == *major && min >= *minor),
                    format!(
                        "Requested api-server version {major}.{minor} is over current version {maj}.{min}. Please upgrade your cluster first"
                    ),
                    15 * 60,
                ))
            }
            VynilPackageRequirement::CustomResourceDefinition(crd) => {
                let api: Api<CustomResourceDefinition> = Api::all(client);
                let r = api.get_metadata_opt(crd).await.map_err(Error::KubeError)?;
                Ok((r.is_some(), format!("CRD {crd} is not installed"), 5 * 60))
            }
            VynilPackageRequirement::Prefly { script, name } => {
                let mut rhai = Script::new(vec![]);
                rhai.ctx.set_value("instance", inst.clone());
                Ok((
                    rhai.eval_truth(script)?,
                    format!("Requirement {name} failed"),
                    5 * 60,
                ))
            }
            VynilPackageRequirement::SystemPackage { category, name } => {
                let api: Api<SystemInstance> = Api::all(client);
                let lst = api.list(&ListParams::default()).await.map_err(Error::KubeError)?;
                Ok((
                    lst.items
                        .into_iter()
                        .any(|i| i.spec.category == *category && i.spec.package == *name),
                    format!("System package {category}/{name} is not installed"),
                    15 * 60,
                ))
            }
            VynilPackageRequirement::TenantPackage { category, name } => {
                let allowed = inst.get_tenant_namespaces().await?;
                let api: Api<TenantInstance> = Api::all(client);
                let lst = api.list(&ListParams::default()).await.map_err(Error::KubeError)?;
                Ok((
                    lst.items.into_iter().any(|i| {
                        i.spec.category == *category
                            && i.spec.package == *name
                            && allowed.contains(&i.metadata.namespace.unwrap())
                    }),
                    format!("Tenant package {category}/{name} is not installed"),
                    15 * 60,
                ))
            }
            VynilPackageRequirement::TenantService(svc) => Ok((
                inst.get_tenant_services_names()
                    .await?
                    .into_iter()
                    .any(|i| i == *svc),
                format!("Tenant service {svc} is not installed"),
                15 * 60,
            )),
            VynilPackageRequirement::StorageCapability(capa) => {
                //TODO: implement StorageCapability
                tracing::warn!("StorageCapability Requirement is a TODO");
                Ok((
                    true,
                    format!("Storage capability {:?} isn't available", capa),
                    15 * 60,
                ))
            }
            VynilPackageRequirement::MinimumPreviousVersion(prev) => {
                //TODO: implement MinimumPreviousVersion
                tracing::warn!("MinimumPreviousVersion Requirement is a TODO");
                Ok((true, format!("Minimum {prev} version is not available"), 15 * 60))
            }
            VynilPackageRequirement::SystemService(svc) => {
                let lst = ServiceInstance::get_all_services_names().await?;
                Ok((
                    lst.iter().any(|i| svc == i),
                    format!("System Service {svc} is not available"),
                    15 * 60,
                ))
            }
            _ => Ok((true, "".to_string(), 15 * 60)),
        }
    }

    pub async fn check_service(&self, inst: &ServiceInstance, client: Client) -> Result<(bool, String, u64)> {
        match self {
            VynilPackageRequirement::VynilVersion(v) => {
                let requested = Semver::parse(v)?;
                let current = Semver::parse(VERSION)?;
                Ok((
                    current >= requested,
                    format!(
                        "Requested vynil version {v} is over current version {VERSION}. Please upgrade vynil first"
                    ),
                    15 * 60,
                ))
            }
            VynilPackageRequirement::ClusterVersion { major, minor } => {
                let raw = crate::k8sraw::K8sRaw::new();
                let ver = raw.get_api_version().await?;
                let maj: u64 = serde_json::to_string(&ver.as_object().unwrap()["major"])
                    .map_err(Error::SerializationError)?
                    .parse()
                    .map_err(Error::ParseInt)?;
                let min: u64 = serde_json::to_string(&ver.as_object().unwrap()["minor"])
                    .map_err(Error::SerializationError)?
                    .parse()
                    .map_err(Error::ParseInt)?;
                Ok((
                    maj > *major || (maj == *major && min >= *minor),
                    format!(
                        "Requested api-server version {major}.{minor} is over current version {maj}.{min}. Please upgrade your cluster first"
                    ),
                    15 * 60,
                ))
            }
            VynilPackageRequirement::CustomResourceDefinition(crd) => {
                let api: Api<CustomResourceDefinition> = Api::all(client);
                let r = api.get_metadata_opt(crd).await.map_err(Error::KubeError)?;
                Ok((r.is_some(), format!("CRD {crd} is not installed"), 5 * 60))
            }
            VynilPackageRequirement::Prefly { script, name } => {
                let mut rhai = Script::new(vec![]);
                rhai.ctx.set_value("instance", inst.clone());
                Ok((
                    rhai.eval_truth(script)?,
                    format!("Requirement {name} failed"),
                    5 * 60,
                ))
            }
            VynilPackageRequirement::SystemPackage { category, name } => {
                let api: Api<SystemInstance> = Api::all(client);
                let lst = api.list(&ListParams::default()).await.map_err(Error::KubeError)?;
                Ok((
                    lst.items
                        .into_iter()
                        .any(|i| i.spec.category == *category && i.spec.package == *name),
                    format!("System package {category}/{name} is not installed"),
                    15 * 60,
                ))
            }
            VynilPackageRequirement::StorageCapability(capa) => {
                //TODO: implement StorageCapability
                tracing::warn!("StorageCapability Requirement is a TODO");
                Ok((
                    true,
                    format!("Storage capability {:?} isn't available", capa),
                    15 * 60,
                ))
            }
            VynilPackageRequirement::MinimumPreviousVersion(prev) => {
                //TODO: implement MinimumPreviousVersion
                tracing::warn!("MinimumPreviousVersion Requirement is a TODO");
                Ok((true, format!("Minimum {prev} version is not available"), 15 * 60))
            }
            VynilPackageRequirement::SystemService(svc) => {
                let lst = ServiceInstance::get_all_services_names().await?;
                Ok((
                    lst.iter().any(|i| svc == i),
                    format!("System Service {svc} is not available"),
                    15 * 60,
                ))
            }
            _ => Ok((true, "".to_string(), 15 * 60)),
        }
    }
}


/// Vynil Package Recommandation
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VynilPackageRecommandation {
    /// Name of a crd that is required before installing this package
    CustomResourceDefinition(String),
    /// Name of a System Service that should be installed before current package
    SystemService(String),
    /// Name of a Tenant Service that should be installed before current package
    TenantService(String),
}

/// Vynil Package in JukeBox status
#[derive(Deserialize, Serialize, PartialEq, Clone, Debug, JsonSchema)]
pub struct VynilPackage {
    /// Registry
    pub registry: String,
    /// Image
    pub image: String,
    /// Current tag
    pub tag: String,
    /// Metadata for a package
    pub metadata: VynilPackageMeta,
    /// Requirements
    pub requirements: Vec<VynilPackageRequirement>,
    /// Recommandations
    pub recommandations: Option<Vec<VynilPackageRecommandation>>,
    /// Component options
    pub options: Option<BTreeMap<String, serde_json::Value>>,
    /// A rhai script that produce a map to be added in the package values
    pub value_script: Option<String>,
}
impl VynilPackage {
    pub fn get_min_version(&self) -> Option<String> {
        for rec in &self.requirements {
            match rec {
                VynilPackageRequirement::MinimumPreviousVersion(v) => return Some(v.clone()),
                _ => {}
            }
        }
        None
    }

    pub fn get_vynil_version(&self) -> Option<String> {
        for rec in &self.requirements {
            match rec {
                VynilPackageRequirement::VynilVersion(v) => return Some(v.clone()),
                _ => {}
            }
        }
        None
    }

    pub fn get_cluster_version(&self) -> Option<(u64, u64)> {
        for rec in &self.requirements {
            match rec {
                VynilPackageRequirement::ClusterVersion { major, minor } => return Some((*major, *minor)),
                _ => {}
            }
        }
        None
    }

    pub fn is_min_version_ok(&self, current: String) -> bool {
        let parse = Semver::parse(&current);
        if parse.is_ok() {
            let cur = parse.unwrap();
            if let Some(target) = self.get_min_version() {
                let target_parsed = Semver::parse(&target);
                if target_parsed.is_ok() {
                    cur >= target_parsed.unwrap()
                } else {
                    true
                }
            } else {
                true
            }
        } else {
            true
        }
    }

    pub fn is_vynil_version_ok(&self) -> bool {
        let parse = Semver::parse(VERSION);
        if parse.is_ok() {
            let cur = parse.unwrap();
            if let Some(target) = self.get_vynil_version() {
                let target_parsed = Semver::parse(&target);
                if target_parsed.is_ok() {
                    cur >= target_parsed.unwrap()
                } else {
                    true
                }
            } else {
                true
            }
        } else {
            true
        }
    }

    pub fn is_cluster_version_ok(&self) -> Result<bool> {
        let raw = crate::k8sraw::K8sRaw::new();
        let ver = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move { raw.get_api_version().await })
        })?;
        let maj: u64 = serde_json::to_string(&ver.as_object().unwrap()["major"])
            .map_err(Error::SerializationError)?
            .parse()
            .map_err(Error::ParseInt)?;
        let min: u64 = serde_json::to_string(&ver.as_object().unwrap()["minor"])
            .map_err(Error::SerializationError)?
            .parse()
            .map_err(Error::ParseInt)?;
        if let Some((major, minor)) = self.get_cluster_version() {
            Ok(maj > major || (maj == major && min >= minor))
        } else {
            Ok(true)
        }
    }
}

/// Image definitions
#[derive(Deserialize, Serialize, PartialEq, Clone, Debug)]
pub struct Image {
    /// Current tag
    pub tag: Option<String>,
    /// Metadata for a package
    pub registry: String,
    /// Requirement
    pub repository: String,
}

/// Resource item definitions
#[derive(Deserialize, Serialize, PartialEq, Clone, Debug)]
pub struct ResourceItem {
    /// Cpu ressource
    pub cpu: Option<String>,
    /// Memory ressource
    pub memory: Option<String>,
    /// Storage ressource
    pub storage: Option<String>,
}

/// Resource scaler definitions
#[derive(Deserialize, Serialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct ResourceScaler {
    /// Maximum replicas count
    pub max_replicas: u64,
    /// average cpu utilization
    pub average_utilization: u64,
}

/// Resource definition definitions
#[derive(Deserialize, Serialize, PartialEq, Clone, Debug)]
pub struct Resource {
    /// Ressources requests
    pub requests: Option<ResourceItem>,
    /// Ressources limits
    pub limits: Option<ResourceItem>,
    /// Ressources scaler
    pub scaler: Option<ResourceScaler>,
}


/// Vynil Package in the sources
#[allow(non_snake_case)]
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct VynilPackageSource {
    pub apiVersion: String,
    pub kind: String,
    /// Metadata for a package
    pub metadata: VynilPackageMeta,
    /// Requirement
    pub requirements: Vec<VynilPackageRequirement>,
    /// Recommandations
    pub recommandations: Option<Vec<VynilPackageRecommandation>>,
    /// Component options
    pub options: Option<BTreeMap<String, serde_json::Value>>,
    /// Images definition
    pub images: Option<BTreeMap<String, Image>>,
    /// Images definition
    pub resources: Option<BTreeMap<String, Resource>>,
    /// A rhai script that produce a map to be added in the package values
    pub value_script: Option<String>,
}
impl VynilPackageSource {
    pub fn get_metadata(&mut self) -> RhaiRes<Dynamic> {
        let v = serde_json::to_string(&self.metadata)
            .map_err(Error::JsonError)
            .map_err(rhai_err)?;
        serde_json::from_str(&v)
            .map_err(Error::JsonError)
            .map_err(rhai_err)
    }

    pub fn get_requirements(&mut self) -> RhaiRes<Dynamic> {
        let v = serde_json::to_string(&self.requirements)
            .map_err(Error::JsonError)
            .map_err(rhai_err)?;
        serde_json::from_str(&v)
            .map_err(Error::JsonError)
            .map_err(rhai_err)
    }

    pub fn get_recommandations(&mut self) -> RhaiRes<Dynamic> {
        if let Some(recos) = self.recommandations.clone() {
            let v = serde_json::to_string(&recos)
                .map_err(Error::JsonError)
                .map_err(rhai_err)?;
            serde_json::from_str(&v)
                .map_err(Error::JsonError)
                .map_err(rhai_err)
        } else {
            Ok(Dynamic::from(()))
        }
    }

    pub fn get_options(&mut self) -> RhaiRes<Dynamic> {
        if let Some(opt) = self.options.clone() {
            let v = serde_json::to_string(&opt)
                .map_err(Error::JsonError)
                .map_err(rhai_err)?;
            serde_json::from_str(&v)
                .map_err(Error::JsonError)
                .map_err(rhai_err)
        } else {
            Ok(Dynamic::from(()))
        }
    }

    pub fn get_value_script(&mut self) -> RhaiRes<String> {
        if let Some(val) = self.value_script.clone() {
            Ok(val)
        } else {
            Ok("".into())
        }
    }

    pub fn get_images(&mut self) -> RhaiRes<Dynamic> {
        if let Some(opt) = self.images.clone() {
            let v = serde_json::to_string(&opt)
                .map_err(Error::JsonError)
                .map_err(rhai_err)?;
            serde_json::from_str(&v)
                .map_err(Error::JsonError)
                .map_err(rhai_err)
        } else {
            Ok(Dynamic::from(()))
        }
    }

    pub fn get_resources(&mut self) -> RhaiRes<Dynamic> {
        if let Some(opt) = self.resources.clone() {
            let v = serde_json::to_string(&opt)
                .map_err(Error::JsonError)
                .map_err(rhai_err)?;
            serde_json::from_str(&v)
                .map_err(Error::JsonError)
                .map_err(rhai_err)
        } else {
            Ok(Dynamic::from(()))
        }
    }

    pub fn validate_options(&mut self) -> RhaiRes<()> {
        if let Some(options) = self.options.clone() {
            for (_key, val) in &options {
                let _schema: &Schema = &serde_json::from_str(serde_json::to_string(val).unwrap().as_str())
                    .map_err(Error::JsonError)
                    .map_err(rhai_err)?;
            }
        }
        Ok(())
    }
}

pub fn read_package_yaml(file: &PathBuf) -> Result<VynilPackageSource> {
    let content = fs::read_to_string(Path::new(&file)).map_err(Error::Stdio)?;
    let yaml_value = rust_yaml::Yaml::new()
        .load_str(&content)
        .map_err(|e| Error::YamlError(e.to_string()))?;
    let json_value = crate::yamlhandler::yaml_value_to_serde_json(yaml_value);
    serde_json::from_value(json_value).map_err(Error::SerializationError)
}
pub fn rhai_read_package_yaml(file: String) -> RhaiRes<VynilPackageSource> {
    read_package_yaml(&PathBuf::from(&file)).map_err(rhai_err)
}

pub fn package_rhai_register(engine: &mut Engine) {
    engine
        .register_fn("vynil_version", get_vynil_version)
        .register_type_with_name::<VynilPackageSource>("VynilPackage")
        .register_fn("read_package_yaml", rhai_read_package_yaml)
        .register_fn("validate_options", VynilPackageSource::validate_options)
        .register_get("metadata", VynilPackageSource::get_metadata)
        .register_get("requirements", VynilPackageSource::get_requirements)
        .register_get("recommandations", VynilPackageSource::get_recommandations)
        .register_get("options", VynilPackageSource::get_options)
        .register_get("value_script", VynilPackageSource::get_value_script)
        .register_get("images", VynilPackageSource::get_images)
        .register_get("resources", VynilPackageSource::get_resources);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Write YAML content to a unique temp file and return its path.
    fn write_temp_yaml(content: &str, tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "vynil_test_{}_{}.yaml",
            std::process::id(),
            tag
        ));
        std::fs::write(&path, content).unwrap();
        path
    }

    // ── Minimal package ───────────────────────────────────────────────────────

    const MINIMAL_YAML: &str = "\
apiVersion: vynil.solidite.fr/v1
kind: Package
metadata:
  name: test-pkg
  category: apps
  description: A minimal test package
  type: tenant
  features:
    - upgrade
requirements: []
";

    #[test]
    fn test_read_package_yaml_minimal_fields() {
        let p = write_temp_yaml(MINIMAL_YAML, "minimal");
        let pkg = read_package_yaml(&p).unwrap();
        assert_eq!(pkg.metadata.name, "test-pkg");
        assert_eq!(pkg.metadata.category, "apps");
        assert_eq!(pkg.metadata.description, "A minimal test package");
        assert!(matches!(pkg.metadata.usage, VynilPackageType::Tenant));
        assert!(pkg.requirements.is_empty());
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn test_read_package_yaml_system_type() {
        let yaml = MINIMAL_YAML.replace("type: tenant", "type: system");
        let p = write_temp_yaml(&yaml, "systype");
        let pkg = read_package_yaml(&p).unwrap();
        assert!(matches!(pkg.metadata.usage, VynilPackageType::System));
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn test_read_package_yaml_service_type() {
        let yaml = MINIMAL_YAML.replace("type: tenant", "type: service");
        let p = write_temp_yaml(&yaml, "svctype");
        let pkg = read_package_yaml(&p).unwrap();
        assert!(matches!(pkg.metadata.usage, VynilPackageType::Service));
        std::fs::remove_file(p).ok();
    }

    // ── Requirement enum variants ─────────────────────────────────────────────

    const REQUIREMENTS_YAML: &str = "\
apiVersion: vynil.solidite.fr/v1
kind: Package
metadata:
  name: full-pkg
  category: apps
  description: Full requirements test
  type: tenant
  features:
    - upgrade
requirements:
  - custom_resource_definition: some.crd.io
  - system_service: monitoring
  - tenant_service: auth
  - system_package:
      category: storage
      name: longhorn
  - tenant_package:
      category: infra
      name: postgres
  - vynil_version: \"0.5.0\"
  - minimum_previous_version: \"0.4.0\"
  - cluster_version:
      major: 1
      minor: 25
  - cpu: 2.0
  - memory: 512
  - disk: 1024
";

    #[test]
    fn test_requirement_custom_resource_definition() {
        let p = write_temp_yaml(REQUIREMENTS_YAML, "req_crd");
        let pkg = read_package_yaml(&p).unwrap();
        assert!(matches!(
            &pkg.requirements[0],
            VynilPackageRequirement::CustomResourceDefinition(s) if s == "some.crd.io"
        ));
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn test_requirement_system_service() {
        let p = write_temp_yaml(REQUIREMENTS_YAML, "req_svc");
        let pkg = read_package_yaml(&p).unwrap();
        assert!(matches!(
            &pkg.requirements[1],
            VynilPackageRequirement::SystemService(s) if s == "monitoring"
        ));
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn test_requirement_tenant_service() {
        let p = write_temp_yaml(REQUIREMENTS_YAML, "req_tsvc");
        let pkg = read_package_yaml(&p).unwrap();
        assert!(matches!(
            &pkg.requirements[2],
            VynilPackageRequirement::TenantService(s) if s == "auth"
        ));
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn test_requirement_system_package() {
        let p = write_temp_yaml(REQUIREMENTS_YAML, "req_spkg");
        let pkg = read_package_yaml(&p).unwrap();
        assert!(matches!(
            &pkg.requirements[3],
            VynilPackageRequirement::SystemPackage { category, name }
            if category == "storage" && name == "longhorn"
        ));
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn test_requirement_tenant_package() {
        let p = write_temp_yaml(REQUIREMENTS_YAML, "req_tpkg");
        let pkg = read_package_yaml(&p).unwrap();
        assert!(matches!(
            &pkg.requirements[4],
            VynilPackageRequirement::TenantPackage { category, name }
            if category == "infra" && name == "postgres"
        ));
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn test_requirement_vynil_version() {
        let p = write_temp_yaml(REQUIREMENTS_YAML, "req_vynil");
        let pkg = read_package_yaml(&p).unwrap();
        assert!(matches!(
            &pkg.requirements[5],
            VynilPackageRequirement::VynilVersion(v) if v == "0.5.0"
        ));
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn test_requirement_minimum_previous_version() {
        let p = write_temp_yaml(REQUIREMENTS_YAML, "req_minprev");
        let pkg = read_package_yaml(&p).unwrap();
        assert!(matches!(
            &pkg.requirements[6],
            VynilPackageRequirement::MinimumPreviousVersion(v) if v == "0.4.0"
        ));
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn test_requirement_cluster_version() {
        let p = write_temp_yaml(REQUIREMENTS_YAML, "req_clver");
        let pkg = read_package_yaml(&p).unwrap();
        assert!(matches!(
            &pkg.requirements[7],
            VynilPackageRequirement::ClusterVersion { major: 1, minor: 25 }
        ));
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn test_requirement_cpu() {
        let p = write_temp_yaml(REQUIREMENTS_YAML, "req_cpu");
        let pkg = read_package_yaml(&p).unwrap();
        assert!(matches!(
            &pkg.requirements[8],
            VynilPackageRequirement::Cpu(c) if (*c - 2.0_f64).abs() < f64::EPSILON
        ));
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn test_requirement_memory_and_disk() {
        let p = write_temp_yaml(REQUIREMENTS_YAML, "req_mem");
        let pkg = read_package_yaml(&p).unwrap();
        assert!(matches!(&pkg.requirements[9], VynilPackageRequirement::Memory(512)));
        assert!(matches!(&pkg.requirements[10], VynilPackageRequirement::Disk(1024)));
        std::fs::remove_file(p).ok();
    }

    // ── Diagnostic test (temporary) ──────────────────────────────────────────

    #[test]
    fn debug_yaml_to_json_conversion() {
        // Test 1: nested block mapping alone
        let y1 = "images:\n  main:\n    registry: docker.io\n";
        let v1 = rust_yaml::Yaml::new().load_str(y1).unwrap();
        let j1 = crate::yamlhandler::yaml_value_to_serde_json(v1);
        eprintln!("Test1 (nested block alone): {}", serde_json::to_string_pretty(&j1).unwrap());

        // Test 2: nested block mapping after simple key
        let y2 = "kind: Package\nimages:\n  main:\n    registry: docker.io\n";
        let v2 = rust_yaml::Yaml::new().load_str(y2).unwrap();
        let j2 = crate::yamlhandler::yaml_value_to_serde_json(v2);
        eprintln!("Test2 (after simple key): {}", serde_json::to_string_pretty(&j2).unwrap());

        // Test 3: nested block mapping after flow value
        let y3 = "requirements: []\nimages:\n  main:\n    registry: docker.io\n";
        let v3 = rust_yaml::Yaml::new().load_str(y3).unwrap();
        let j3 = crate::yamlhandler::yaml_value_to_serde_json(v3);
        eprintln!("Test3 (after flow value): {}", serde_json::to_string_pretty(&j3).unwrap());

        // Test 4: flow images
        let y4 = "requirements: []\nimages: {main: {registry: docker.io, repository: lib/nginx, tag: '1.25'}}\n";
        let v4 = rust_yaml::Yaml::new().load_str(y4).unwrap();
        let j4 = crate::yamlhandler::yaml_value_to_serde_json(v4);
        eprintln!("Test4 (all flow): {}", serde_json::to_string_pretty(&j4).unwrap());
    }

    // ── Images & resources ────────────────────────────────────────────────────

    #[test]
    fn test_read_package_yaml_with_images() {
        let yaml = "\
apiVersion: vynil.solidite.fr/v1
kind: Package
metadata:
  name: img-pkg
  category: apps
  description: Package with images
  type: tenant
  features: []
requirements: []
images:
  main:
    registry: docker.io
    repository: library/nginx
    tag: \"1.25\"
  sidecar:
    registry: gcr.io
    repository: distroless/base
";
        let p = write_temp_yaml(yaml, "images");
        let pkg = read_package_yaml(&p).unwrap();
        let images = pkg.images.unwrap();
        assert!(images.contains_key("main"));
        assert_eq!(images["main"].registry, "docker.io");
        assert_eq!(images["main"].repository, "library/nginx");
        assert_eq!(images["main"].tag, Some("1.25".to_string()));
        assert!(images.contains_key("sidecar"));
        assert_eq!(images["sidecar"].tag, None);
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn test_read_package_yaml_with_resources() {
        let yaml = "\
apiVersion: vynil.solidite.fr/v1
kind: Package
metadata:
  name: res-pkg
  category: apps
  description: Package with resources
  type: tenant
  features: []
requirements: []
resources:
  app:
    requests:
      cpu: 100m
      memory: 128Mi
    limits:
      cpu: 500m
      memory: 512Mi
";
        let p = write_temp_yaml(yaml, "resources");
        let pkg = read_package_yaml(&p).unwrap();
        let resources = pkg.resources.unwrap();
        assert!(resources.contains_key("app"));
        let req = resources["app"].requests.as_ref().unwrap();
        assert_eq!(req.cpu, Some("100m".to_string()));
        assert_eq!(req.memory, Some("128Mi".to_string()));
        std::fs::remove_file(p).ok();
    }

    // ── VynilPackage methods ──────────────────────────────────────────────────

    fn make_package(requirements: Vec<VynilPackageRequirement>) -> VynilPackage {
        VynilPackage {
            registry: "docker.io".into(),
            image: "test/image".into(),
            tag: "1.0.0".into(),
            metadata: VynilPackageMeta {
                name: "test".into(),
                category: "test".into(),
                description: "test package".into(),
                app_version: None,
                usage: VynilPackageType::Tenant,
                features: vec![],
            },
            requirements,
            recommandations: None,
            options: None,
            value_script: None,
        }
    }

    #[test]
    fn test_is_min_version_ok_above_minimum() {
        let pkg = make_package(vec![VynilPackageRequirement::MinimumPreviousVersion(
            "1.0.0".into(),
        )]);
        assert!(pkg.is_min_version_ok("1.0.1".into()));
        assert!(pkg.is_min_version_ok("2.0.0".into()));
    }

    #[test]
    fn test_is_min_version_ok_at_minimum() {
        let pkg = make_package(vec![VynilPackageRequirement::MinimumPreviousVersion(
            "1.0.0".into(),
        )]);
        assert!(pkg.is_min_version_ok("1.0.0".into()));
    }

    #[test]
    fn test_is_min_version_ok_below_minimum() {
        let pkg = make_package(vec![VynilPackageRequirement::MinimumPreviousVersion(
            "1.0.0".into(),
        )]);
        assert!(!pkg.is_min_version_ok("0.9.9".into()));
    }

    #[test]
    fn test_is_min_version_ok_no_requirement() {
        let pkg = make_package(vec![]);
        // No MinimumPreviousVersion → always OK
        assert!(pkg.is_min_version_ok("0.1.0".into()));
    }

    #[test]
    fn test_get_min_version_present() {
        let pkg = make_package(vec![
            VynilPackageRequirement::VynilVersion("0.5.0".into()),
            VynilPackageRequirement::MinimumPreviousVersion("1.2.0".into()),
        ]);
        assert_eq!(pkg.get_min_version(), Some("1.2.0".into()));
    }

    #[test]
    fn test_get_min_version_absent() {
        let pkg = make_package(vec![VynilPackageRequirement::VynilVersion("0.5.0".into())]);
        assert_eq!(pkg.get_min_version(), None);
    }

    #[test]
    fn test_get_vynil_version_present() {
        let pkg = make_package(vec![VynilPackageRequirement::VynilVersion("0.5.0".into())]);
        assert_eq!(pkg.get_vynil_version(), Some("0.5.0".into()));
    }

    #[test]
    fn test_get_vynil_version_absent() {
        let pkg = make_package(vec![]);
        assert_eq!(pkg.get_vynil_version(), None);
    }

    #[test]
    fn test_get_cluster_version_present() {
        let pkg = make_package(vec![VynilPackageRequirement::ClusterVersion {
            major: 1,
            minor: 28,
        }]);
        assert_eq!(pkg.get_cluster_version(), Some((1, 28)));
    }

    #[test]
    fn test_read_package_yaml_missing_file() {
        let result = read_package_yaml(&PathBuf::from("/nonexistent/path/pkg.yaml"));
        assert!(result.is_err());
    }
}
