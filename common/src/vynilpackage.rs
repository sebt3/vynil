use crate::{
    instancesystem::SystemInstance, instancetenant::TenantInstance, rhai_err, rhaihandler::Script, Error,
    Result, RhaiRes,
};
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::{api::ListParams, Api, Client};
pub use openapiv3::Schema;
use rhai::Dynamic;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

/// Vynil package type
#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum VynilPackageType {
    #[default]
    Tenant,
    System,
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
    CustomResourceDefinition(String),
    SystemPackage { category: String, name: String },
    TenantPackage { category: String, name: String },
    Prefly { script: String, name: String }, // a rhai script that return a boolean
    StorageCapability(StorageCapability),
    MinimumPreviousVersion(String), // Forbid migration that are not supported
    // A titre informatif pour l'utilisateur
    Cpu(f64),    // Sum of all requests
    Memory(u64), // MB, Sum of all requests
    Disk(u64),   // MB, Sum of all requests
}
impl VynilPackageRequirement {
    pub async fn check_system(&self, inst: &SystemInstance, client: Client) -> Result<(bool, String)> {
        match self {
            VynilPackageRequirement::CustomResourceDefinition(crd) => {
                let api: Api<CustomResourceDefinition> = Api::all(client);
                let r = api.get_metadata_opt(crd).await.map_err(|e| Error::KubeError(e))?;
                Ok((r.is_some(), format!("CRD {crd} is not installed")))
            }
            VynilPackageRequirement::Prefly { script, name } => {
                let mut rhai = Script::new(vec![]);
                rhai.ctx.set_value("instance", inst.clone());
                Ok((rhai.eval_truth(&script)?, format!("Requirement {name} failed")))
            }
            VynilPackageRequirement::SystemPackage { category, name } => {
                let api: Api<SystemInstance> = Api::all(client);
                let lst = api
                    .list(&ListParams::default())
                    .await
                    .map_err(|e| Error::KubeError(e))?;
                Ok((
                    lst.items
                        .into_iter()
                        .any(|i| i.spec.category == *category && i.spec.package == *name),
                    format!("System package {category}/{name} is not installed"),
                ))
            }
            VynilPackageRequirement::TenantPackage { category, name } => {
                tracing::warn!("TenantPackage Requirement for a system package is invalid, skipping");
                Ok((true, format!("Tenant package {category}/{name} is not installed")))
            }
            VynilPackageRequirement::StorageCapability(capa) => {
                //TODO: implement StorageCapability
                tracing::warn!("StorageCapability Requirement is a TODO");
                Ok((true, format!("Storage capability {:?} isn't available", capa)))
            }
            VynilPackageRequirement::MinimumPreviousVersion(prev) => {
                //TODO: implement MinimumPreviousVersion
                tracing::warn!("MinimumPreviousVersion Requirement is a TODO");
                Ok((true, format!("Minimum {prev} version is not available")))
            }
            _ => Ok((true, "".to_string())),
        }
    }

    pub async fn check_tenant(&self, inst: &TenantInstance, client: Client) -> Result<(bool, String)> {
        match self {
            VynilPackageRequirement::CustomResourceDefinition(crd) => {
                let api: Api<CustomResourceDefinition> = Api::all(client);
                let r = api.get_metadata_opt(crd).await.map_err(|e| Error::KubeError(e))?;
                Ok((r.is_some(), format!("CRD {crd} is not installed")))
            }
            VynilPackageRequirement::Prefly { script, name } => {
                let mut rhai = Script::new(vec![]);
                rhai.ctx.set_value("instance", inst.clone());
                Ok((rhai.eval_truth(&script)?, format!("Requirement {name} failed")))
            }
            VynilPackageRequirement::SystemPackage { category, name } => {
                let api: Api<SystemInstance> = Api::all(client);
                let lst = api
                    .list(&ListParams::default())
                    .await
                    .map_err(|e| Error::KubeError(e))?;
                Ok((
                    lst.items
                        .into_iter()
                        .any(|i| i.spec.category == *category && i.spec.package == *name),
                    format!("System package {category}/{name} is not installed"),
                ))
            }
            VynilPackageRequirement::TenantPackage { category, name } => {
                let allowed = inst.get_tenant_namespaces().await?;
                let api: Api<TenantInstance> = Api::all(client);
                let lst = api
                    .list(&ListParams::default())
                    .await
                    .map_err(|e| Error::KubeError(e))?;
                Ok((
                    lst.items.into_iter().any(|i| {
                        i.spec.category == *category
                            && i.spec.package == *name
                            && allowed.contains(&i.metadata.namespace.unwrap())
                    }),
                    format!("Tenant package {category}/{name} is not installed"),
                ))
            }
            VynilPackageRequirement::StorageCapability(capa) => {
                //TODO: implement StorageCapability
                tracing::warn!("StorageCapability Requirement is a TODO");
                Ok((true, format!("Storage capability {:?} isn't available", capa)))
            }
            VynilPackageRequirement::MinimumPreviousVersion(prev) => {
                //TODO: implement MinimumPreviousVersion
                tracing::warn!("MinimumPreviousVersion Requirement is a TODO");
                Ok((true, format!("Minimum {prev} version is not available")))
            }
            _ => Ok((true, "".to_string())),
        }
    }
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
    /// Requirement
    pub requirements: Vec<VynilPackageRequirement>,
    /// Component options
    pub options: Option<BTreeMap<String, serde_json::Value>>,
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
    /// Current tag
    pub cpu: Option<String>,
    /// Metadata for a package
    pub memory: Option<String>,
}

/// Resource definition definitions
#[derive(Deserialize, Serialize, PartialEq, Clone, Debug)]
pub struct Resource {
    /// Current tag
    pub requests: Option<ResourceItem>,
    /// Metadata for a package
    pub limits: Option<ResourceItem>,
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
    /// Component options
    pub options: Option<BTreeMap<String, serde_json::Value>>,
    /// Images definition
    pub images: Option<BTreeMap<String, Image>>,
    /// Images definition
    pub resources: Option<BTreeMap<String, Resource>>,
    // TODO: config externe
}
impl VynilPackageSource {
    pub fn get_metadata(&mut self) -> RhaiRes<Dynamic> {
        let v = serde_json::to_string(&self.metadata).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }

    pub fn get_requirements(&mut self) -> RhaiRes<Dynamic> {
        let v =
            serde_json::to_string(&self.requirements).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }

    pub fn get_options(&mut self) -> RhaiRes<Dynamic> {
        if let Some(opt) = self.options.clone() {
            let v = serde_json::to_string(&opt).map_err(|e| rhai_err(Error::SerializationError(e)))?;
            serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
        } else {
            Ok(Dynamic::from({}))
        }
    }

    pub fn get_images(&mut self) -> RhaiRes<Dynamic> {
        if let Some(opt) = self.images.clone() {
            let v = serde_json::to_string(&opt).map_err(|e| rhai_err(Error::SerializationError(e)))?;
            serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
        } else {
            Ok(Dynamic::from({}))
        }
    }

    pub fn get_resources(&mut self) -> RhaiRes<Dynamic> {
        if let Some(opt) = self.resources.clone() {
            let v = serde_json::to_string(&opt).map_err(|e| rhai_err(Error::SerializationError(e)))?;
            serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
        } else {
            Ok(Dynamic::from({}))
        }
    }

    pub fn validate_options(&mut self) -> RhaiRes<()> {
        if let Some(options) = self.options.clone() {
            for (_key, val) in &options {
                let _schema: &Schema = &serde_json::from_str(serde_json::to_string(val).unwrap().as_str())
                    .map_err(|e| rhai_err(Error::SerializationError(e)))?;
            }
        }
        Ok(())
    }
}

pub fn read_package_yaml(file: &PathBuf) -> Result<VynilPackageSource> {
    let f = fs::File::open(Path::new(&file)).map_err(|e| Error::Stdio(e))?;
    let deserializer = serde_yaml::Deserializer::from_reader(f);
    serde_yaml::with::singleton_map_recursive::deserialize(deserializer).map_err(|e| Error::YamlError(e))
}
pub fn rhai_read_package_yaml(file: String) -> RhaiRes<VynilPackageSource> {
    read_package_yaml(&PathBuf::from(&file)).map_err(|e| rhai_err(e))
}
