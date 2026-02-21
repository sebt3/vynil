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
    kind = "ServiceInstance",
    status = "ServiceInstanceStatus",
    shortname = "vsvc",
    group = "vynil.solidite.fr",
    version = "v1",
    namespaced
)]
#[kube(
    doc = "Custom resource representing an Vynil service package installation",
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
pub struct ServiceInstanceSpec {
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
    CrdApplied,
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
impl_condition_crds!();

/// The status object of `ServiceInstance`
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct ServiceInstanceStatus {
    /// ServiceInstance Conditions
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
    /// List of crds children
    pub crds: Option<Vec<String>>,
    /// List of the services
    pub services: Option<Vec<Published>>,
}

impl ServiceInstance {
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
            if let Some(child) = status.crds.clone() {
                if child.len() > 0 {
                    return true;
                }
            }
        }
        false
    }

    pub async fn get_all_services_names() -> Result<Vec<String>> {
        let client = get_client_async().await;
        let mut list: Vec<String> = Vec::new();
        let lp = ListParams::default();
        for ns in Api::<Namespace>::all(client.clone())
            .list(&lp)
            .await
            .map_err(Error::KubeError)?
            .iter()
            .map(|ns| ns.name_any())
        {
            let api = Api::<Self>::namespaced(client.clone(), &ns);
            api.list(&lp)
                .await
                .map_err(Error::KubeError)?
                .iter()
                .map(|i| {
                    let res: Vec<String> = i.get_services().iter().map(|s| s.key.clone()).collect();
                    res
                })
                .for_each(|mut l| {
                    list.append(&mut l);
                });
        }
        list.sort();
        Ok(list)
    }

    pub fn rhai_list_services_names() -> RhaiRes<Vec<String>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async move { Self::get_all_services_names().await })
        })
        .map_err(rhai_err)
    }
}

impl_instance_common!(ServiceInstance, "ServiceInstance");
impl_instance_crds!(ServiceInstance);
impl_instance_befores!(ServiceInstance);

pub fn service_rhai_register(engine: &mut Engine) {
    engine
            .register_type_with_name::<ServiceInstance>("ServiceInstance")
            .register_fn("get_service_instance", ServiceInstance::rhai_get)
            .register_fn("list_service_instance", ServiceInstance::rhai_list)
            .register_fn("list_services_names", ServiceInstance::rhai_list_services_names)
            .register_fn("options_digest", ServiceInstance::get_options_digest)
            .register_fn("get_tfstate", ServiceInstance::rhai_get_tfstate)
            .register_fn("get_rhaistate", ServiceInstance::rhai_get_rhaistate)
            .register_fn("set_services", ServiceInstance::rhai_set_services)
            .register_fn("get_services", ServiceInstance::rhai_get_services)
            .register_fn("set_agent_started", ServiceInstance::rhai_set_agent_started)
            .register_fn("set_missing_box", ServiceInstance::rhai_set_missing_box)
            .register_fn("set_missing_package", ServiceInstance::rhai_set_missing_package)
            .register_fn(
                "set_missing_requirement",
                ServiceInstance::rhai_set_missing_requirement,
            )
            .register_fn("set_status_ready", ServiceInstance::rhai_set_status_ready)
            .register_fn("set_status_crds", ServiceInstance::rhai_set_status_crds)
            .register_fn(
                "set_status_crd_failed",
                ServiceInstance::rhai_set_status_crd_failed,
            )
            .register_fn("set_status_befores", ServiceInstance::rhai_set_status_befores)
            .register_fn(
                "set_status_before_failed",
                ServiceInstance::rhai_set_status_before_failed,
            )
            .register_fn("set_status_vitals", ServiceInstance::rhai_set_status_vitals)
            .register_fn(
                "set_status_vital_failed",
                ServiceInstance::rhai_set_status_vital_failed,
            )
            .register_fn("set_status_scalables", ServiceInstance::rhai_set_status_scalables)
            .register_fn(
                "set_status_scalable_failed",
                ServiceInstance::rhai_set_status_scalable_failed,
            )
            .register_fn("set_status_others", ServiceInstance::rhai_set_status_others)
            .register_fn(
                "set_status_other_failed",
                ServiceInstance::rhai_set_status_other_failed,
            )
            .register_fn("set_status_posts", ServiceInstance::rhai_set_status_posts)
            .register_fn(
                "set_status_post_failed",
                ServiceInstance::rhai_set_status_post_failed,
            )
            .register_fn("set_tfstate", ServiceInstance::rhai_set_tfstate)
            .register_fn(
                "set_status_tofu_failed",
                ServiceInstance::rhai_set_status_tofu_failed,
            )
            .register_fn("set_rhaistate", ServiceInstance::rhai_set_rhaistate)
            .register_fn(
                "set_status_rhai_failed",
                ServiceInstance::rhai_set_status_rhai_failed,
            )
            .register_fn(
                "set_status_schedule_backup_failed",
                ServiceInstance::rhai_set_status_schedule_backup_failed,
            )
            .register_fn(
                "set_status_init_failed",
                ServiceInstance::rhai_set_status_init_failed,
            )
            .register_get("metadata", ServiceInstance::get_metadata)
            .register_get("spec", ServiceInstance::get_spec)
            .register_get("status", ServiceInstance::get_status);
}
