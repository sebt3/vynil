use crate::{
    Error, Published, Result, RhaiRes,
    context::get_client_async,
    rhai_err,
};
use chrono::{DateTime, Utc};
use k8s_openapi::api::core::v1::Namespace;
use kube::{
    CustomResource, Resource, ResourceExt,
    api::{Api, ListParams},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use rhai::Engine;

/// InitFrom contains the informations for the backup to use to initialize the installation
#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InitFrom {
    /// Name of the secret containing: AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, BASE_REPO_URL and RESTIC_PASSWORD. Default to "backup-settings"
    pub secret_name: Option<String>,
    /// Path within the bucket containing the backup to use for recovery. Default to "<namespace-name>/<app-slug>"
    pub sub_path: Option<String>,
    /// Snapshot id for restoration
    pub snapshot: String,
}

/// Describe a source of vynil packages jukebox
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    kind = "TenantInstance",
    status = "TenantInstanceStatus",
    shortname = "vti",
    group = "vynil.solidite.fr",
    version = "v1",
    namespaced
)]
#[kube(
    doc = "Custom resource representing an Vynil tenant package installation",
    printcolumn = r#"
    {"name":"Juke",   "type":"string", "description":"JukeBox", "jsonPath":".spec.jukebox"},
    {"name":"cat",    "type":"string", "description":"Category", "jsonPath":".spec.category"},
    {"name":"pkg",    "type":"string", "description":"Package", "jsonPath":".spec.package"},
    {"name":"tag",    "type":"string", "description":"Version", "jsonPath":".status.tag"},
    {"name":"last_updated", "type":"date", "description":"Last update date", "format": "date-time", "jsonPath":".status.conditions[?(@.type == 'Ready')].lastTransitionTime"},
    {"name":"stage",  "type":"string", "description":"Stage", "jsonPath":".status.conditions[-1:].type"},
    {"name":"errors", "type":"string", "description":"Errors", "jsonPath":".status.conditions[?(@.status == 'False')].message"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct TenantInstanceSpec {
    /// The jukebox source name
    pub jukebox: String,
    /// The category name
    pub category: String,
    /// The package name
    pub package: String,
    /// The package version
    pub version: Option<String>,
    /// Init from a previous backup
    pub init_from: Option<InitFrom>,
    /// Parameters
    pub options: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, JsonSchema, Default)]
pub enum ConditionsType {
    #[default]
    Ready,
    Installed,
    Backuped,
    Restored,
    AgentStarted,
    TofuInstalled,
    BeforeApplied,
    VitalApplied,
    ScalableApplied,
    InitFrom,
    ScheduleBackup,
    OtherApplied,
    RhaiApplied,
    PostApplied,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, JsonSchema, Default)]
pub enum ConditionsStatus {
    #[default]
    True,
    False,
}

/// ApplicationCondition contains details about an application condition, which is usually an error or warning
#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationCondition {
    /// LastTransitionTime is the time the condition was last observed
    pub last_transition_time: Option<DateTime<Utc>>,
    /// Message contains human-readable message indicating details about condition
    pub message: String,
    /// Type is an application condition type
    #[serde(rename = "type")]
    pub condition_type: ConditionsType,
    /// Status ("True" or "False") describe if the condition is enbled
    pub status: ConditionsStatus,
    /// Generation for that status
    pub generation: i64,
}

impl_condition_common!();
impl_condition_children!();

/// The status object of `TenantInstance`
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct TenantInstanceStatus {
    /// TenantInstance Conditions
    pub conditions: Vec<ApplicationCondition>,
    /// Current tag
    pub tag: Option<String>,
    /// Options digests
    pub digest: Option<String>,
    /// Current terraform status (gzip+base64)
    pub tfstate: Option<String>,
    /// Current rhai status (gzip+base64) (for custom package information)
    pub rhaistate: Option<String>,
    /// List of before children
    pub befores: Option<Vec<crate::Children>>,
    /// List of vital children
    pub vitals: Option<Vec<crate::Children>>,
    /// List of scalable children
    pub scalables: Option<Vec<crate::Children>>,
    /// List of other children
    pub others: Option<Vec<crate::Children>>,
    /// List of post children
    pub posts: Option<Vec<crate::Children>>,
    /// List of the services
    pub services: Option<Vec<Published>>,
}

impl TenantInstance {
    pub fn have_child(&self) -> bool {
        if let Some(status) = self.status.clone() {
            if status.rhaistate.is_some() {
                return true;
            }
            if status.tfstate.is_some() {
                return true;
            }
            if let Some(child) = status.befores.clone() {
                if child.len() > 0 {
                    return true;
                }
            }
            if let Some(child) = status.vitals.clone() {
                if child.len() > 0 {
                    return true;
                }
            }
            if let Some(child) = status.others.clone() {
                if child.len() > 0 {
                    return true;
                }
            }
            if let Some(child) = status.scalables.clone() {
                if child.len() > 0 {
                    return true;
                }
            }
            if let Some(child) = status.posts.clone() {
                if child.len() > 0 {
                    return true;
                }
            }
        }
        false
    }

    pub async fn get_tenant_name(&self) -> Result<String> {
        let my_ns = self.metadata.namespace.clone().unwrap();
        let ns_api: Api<Namespace> = Api::all(get_client_async().await);
        let my_ns_meta = ns_api.get_metadata(&my_ns).await.map_err(Error::KubeError)?;
        let label_key =
            std::env::var("TENANT_LABEL").unwrap_or_else(|_| "vynil.solidite.fr/tenant".to_string());
        if let Some(labels) = my_ns_meta.metadata.labels.clone() {
            if labels.clone().keys().any(|k| k == &label_key) {
                Ok(labels[&label_key].clone())
            } else {
                Ok(my_ns)
            }
        } else {
            Ok(my_ns)
        }
    }

    pub async fn get_tenant_namespaces(&self) -> Result<Vec<String>> {
        let my_ns = self.metadata.namespace.clone().unwrap();
        let ns_api: Api<Namespace> = Api::all(get_client_async().await);
        let my_ns_meta = ns_api.get_metadata(&my_ns).await.map_err(Error::KubeError)?;
        let label_key =
            std::env::var("TENANT_LABEL").unwrap_or_else(|_| "vynil.solidite.fr/tenant".to_string());
        let res = vec![my_ns];
        if let Some(labels) = my_ns_meta.metadata.labels.clone() {
            if labels.clone().keys().any(|k| k == &label_key) {
                let tenant_name = &labels[&label_key];
                let mut lp = ListParams::default();
                lp = lp.labels(format!("{}=={}", label_key, tenant_name).as_str());
                let my_nss = ns_api.list_metadata(&lp).await.map_err(Error::KubeError)?;
                return Ok(my_nss
                    .items
                    .into_iter()
                    .map(|n| n.metadata.name.unwrap())
                    .collect());
            }
        }
        Ok(res)
    }

    pub async fn get_tenant_services_names(&self) -> Result<Vec<String>> {
        let mut res: Vec<String> = Vec::new();
        let cli = get_client_async().await;
        for ns in self.get_tenant_namespaces().await? {
            let api = Api::<Self>::namespaced(cli.clone(), &ns);
            let lp = ListParams::default();
            for tnt in api.list(&lp).await.map_err(Error::KubeError)? {
                let mut svcs: Vec<String> = tnt.get_services().iter().map(|i| i.key.clone()).collect();
                res.append(&mut svcs);
            }
        }
        res.sort();
        Ok(res)
    }

    pub fn rhai_get_tenant_name(&mut self) -> RhaiRes<String> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async move { self.get_tenant_name().await })
        })
        .map_err(rhai_err)
    }

    pub fn rhai_get_tenant_namespaces(&mut self) -> RhaiRes<rhai::Dynamic> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let arr = self.get_tenant_namespaces().await;
                if arr.is_ok() {
                    let arr = arr.unwrap();
                    let v = serde_json::to_string(&arr).map_err(Error::SerializationError)?;
                    serde_json::from_str::<rhai::Dynamic>(&v).map_err(Error::SerializationError)
                } else {
                    arr.map(|_| rhai::Dynamic::from(""))
                }
            })
        })
        .map_err(rhai_err)
    }

    pub fn rhai_get_tenant_services_names(&mut self) -> RhaiRes<rhai::Dynamic> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let arr = self.get_tenant_services_names().await;
                if arr.is_ok() {
                    let arr = arr.unwrap();
                    let v = serde_json::to_string(&arr).map_err(Error::SerializationError)?;
                    serde_json::from_str::<rhai::Dynamic>(&v).map_err(Error::SerializationError)
                } else {
                    arr.map(|_| rhai::Dynamic::from(""))
                }
            })
        })
        .map_err(rhai_err)
    }
}

impl_instance_common!(TenantInstance, "TenantInstance");
impl_instance_befores!(TenantInstance);

pub fn tenant_rhai_register(engine: &mut Engine) {
    engine
            .register_type_with_name::<TenantInstance>("TenantInstance")
            .register_fn("get_tenant_instance", TenantInstance::rhai_get)
            .register_fn("get_tenant_name", TenantInstance::rhai_get_tenant_name)
            .register_fn(
                "get_tenant_namespaces",
                TenantInstance::rhai_get_tenant_namespaces,
            )
            .register_fn(
                "get_tenant_services_names",
                TenantInstance::rhai_get_tenant_services_names,
            )
            .register_fn("list_tenant_instance", TenantInstance::rhai_list)
            .register_fn("options_digest", TenantInstance::get_options_digest)
            .register_fn("get_tfstate", TenantInstance::rhai_get_tfstate)
            .register_fn("get_rhaistate", TenantInstance::rhai_get_rhaistate)
            .register_fn("set_agent_started", TenantInstance::rhai_set_agent_started)
            .register_fn("set_missing_box", TenantInstance::rhai_set_missing_box)
            .register_fn("set_missing_package", TenantInstance::rhai_set_missing_package)
            .register_fn(
                "set_missing_requirement",
                TenantInstance::rhai_set_missing_requirement,
            )
            .register_fn("set_status_ready", TenantInstance::rhai_set_status_ready)
            .register_fn("set_status_befores", TenantInstance::rhai_set_status_befores)
            .register_fn(
                "set_status_before_failed",
                TenantInstance::rhai_set_status_before_failed,
            )
            .register_fn("set_status_vitals", TenantInstance::rhai_set_status_vitals)
            .register_fn(
                "set_status_vital_failed",
                TenantInstance::rhai_set_status_vital_failed,
            )
            .register_fn("set_status_scalables", TenantInstance::rhai_set_status_scalables)
            .register_fn(
                "set_status_scalable_failed",
                TenantInstance::rhai_set_status_scalable_failed,
            )
            .register_fn("set_status_others", TenantInstance::rhai_set_status_others)
            .register_fn(
                "set_status_other_failed",
                TenantInstance::rhai_set_status_other_failed,
            )
            .register_fn("set_status_posts", TenantInstance::rhai_set_status_posts)
            .register_fn(
                "set_status_post_failed",
                TenantInstance::rhai_set_status_post_failed,
            )
            .register_fn("set_tfstate", TenantInstance::rhai_set_tfstate)
            .register_fn(
                "set_status_tofu_failed",
                TenantInstance::rhai_set_status_tofu_failed,
            )
            .register_fn("set_rhaistate", TenantInstance::rhai_set_rhaistate)
            .register_fn("set_services", TenantInstance::rhai_set_services)
            .register_fn("get_services", TenantInstance::rhai_get_services)
            .register_fn(
                "set_status_rhai_failed",
                TenantInstance::rhai_set_status_rhai_failed,
            )
            .register_fn(
                "set_status_schedule_backup_failed",
                TenantInstance::rhai_set_status_schedule_backup_failed,
            )
            .register_fn(
                "set_status_init_failed",
                TenantInstance::rhai_set_status_init_failed,
            )
            .register_get("metadata", TenantInstance::get_metadata)
            .register_get("spec", TenantInstance::get_spec)
            .register_get("status", TenantInstance::get_status);
}
