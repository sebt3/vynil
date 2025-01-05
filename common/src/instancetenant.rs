use crate::{
    context::{get_client, get_reporter, get_short_name},
    rhai_err,
    tools::{base64_gz_decode, encode_base64_gz},
    Error, Result, RhaiRes,
};
use chrono::{DateTime, Utc};
use k8s_openapi::api::core::v1::Namespace;
use kube::{
    api::{Api, ListParams, ObjectList, Patch, PatchParams},
    runtime::events::{Event, EventType, Recorder},
    Client, CustomResource, Resource, ResourceExt,
};
use rhai::Dynamic;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::{runtime::Handle, task::block_in_place};

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
    VitalApplied,
    ScalableApplied,
    InitFrom,
    ScheduleBackup,
    OtherApplied,
    RhaiApplied,
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
impl ApplicationCondition {
    #[must_use]
    pub fn new(
        message: &str,
        status: ConditionsStatus,
        condition_type: ConditionsType,
        generation: i64,
    ) -> ApplicationCondition {
        ApplicationCondition {
            last_transition_time: Some(chrono::offset::Utc::now()),
            status,
            condition_type,
            message: message.to_string(),
            generation,
        }
    }

    pub fn ready_ok(generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            "Installed succesfully",
            ConditionsStatus::True,
            ConditionsType::Ready,
            generation,
        )
    }

    pub fn ready_ko(generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            "No successful install",
            ConditionsStatus::False,
            ConditionsType::Ready,
            generation,
        )
    }

    pub fn installed_ko(message: &str, generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            message,
            ConditionsStatus::False,
            ConditionsType::Installed,
            generation,
        )
    }

    pub fn installed_ok(generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            "Installed succesfully",
            ConditionsStatus::True,
            ConditionsType::Installed,
            generation,
        )
    }

    pub fn agent_started(generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            "Agent started",
            ConditionsStatus::True,
            ConditionsType::AgentStarted,
            generation,
        )
    }

    pub fn missing_package(cat: &str, name: &str, generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            &format!("Package {cat}/{name} is missing"),
            ConditionsStatus::False,
            ConditionsType::AgentStarted,
            generation,
        )
    }

    pub fn missing_box(name: &str, generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            &format!("TenantInstance {name} is missing"),
            ConditionsStatus::False,
            ConditionsType::AgentStarted,
            generation,
        )
    }

    pub fn missing_requirement(error: &str, generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            error,
            ConditionsStatus::False,
            ConditionsType::AgentStarted,
            generation,
        )
    }

    pub fn tofu_ko(message: &str, generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            message,
            ConditionsStatus::False,
            ConditionsType::TofuInstalled,
            generation,
        )
    }

    pub fn tofu_ok(generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            "Tofu layer applied succesfully",
            ConditionsStatus::True,
            ConditionsType::TofuInstalled,
            generation,
        )
    }

    pub fn vital_ko(message: &str, generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            message,
            ConditionsStatus::False,
            ConditionsType::VitalApplied,
            generation,
        )
    }

    pub fn vital_ok(generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            "vitals templates applied succesfully",
            ConditionsStatus::True,
            ConditionsType::VitalApplied,
            generation,
        )
    }

    pub fn scalable_ko(message: &str, generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            message,
            ConditionsStatus::False,
            ConditionsType::ScalableApplied,
            generation,
        )
    }

    pub fn scalable_ok(generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            "Scalables templates applied succesfully",
            ConditionsStatus::True,
            ConditionsType::ScalableApplied,
            generation,
        )
    }

    pub fn other_ko(message: &str, generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            message,
            ConditionsStatus::False,
            ConditionsType::OtherApplied,
            generation,
        )
    }

    pub fn other_ok(generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            "Templates applied succesfully",
            ConditionsStatus::True,
            ConditionsType::OtherApplied,
            generation,
        )
    }

    pub fn rhai_ko(message: &str, generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            message,
            ConditionsStatus::False,
            ConditionsType::RhaiApplied,
            generation,
        )
    }

    pub fn rhai_ok(generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            "Custom rhai script succeed",
            ConditionsStatus::True,
            ConditionsType::RhaiApplied,
            generation,
        )
    }

    pub fn init_ko(message: &str, generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            message,
            ConditionsStatus::False,
            ConditionsType::InitFrom,
            generation,
        )
    }

    pub fn schedule_ko(message: &str, generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            message,
            ConditionsStatus::False,
            ConditionsType::ScheduleBackup,
            generation,
        )
    }
}


/// Children describe a k8s object
#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Children {
    /// kind of k8s object
    pub kind: String,
    /// Name of the object
    pub name: String,
    /// Namespace is only used for Cluster TenantInstance for namespaced object
    pub namespace: Option<String>,
}

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
    /// List of vital children
    pub vitals: Option<Vec<Children>>,
    /// List of scalable children
    pub scalables: Option<Vec<Children>>,
    /// List of other children
    pub others: Option<Vec<Children>>,
    // TODO: External configs
}

impl TenantInstance {
    pub async fn get(namespace: String, name: String) -> Result<Self> {
        let api = Api::<Self>::namespaced(get_client(), &namespace);
        api.get(&name).await.map_err(Error::KubeError)
    }

    pub async fn list(namespace: String) -> Result<ObjectList<Self>> {
        let api = Api::<Self>::namespaced(get_client(), &namespace);
        let lp = ListParams::default();
        api.list(&lp).await.map_err(Error::KubeError)
    }

    pub fn have_child(&self) -> bool {
        if let Some(status) = self.status.clone() {
            if status.rhaistate.is_some() {
                return true;
            }
            if status.tfstate.is_some() {
                return true;
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
        }
        false
    }

    pub fn get_options_digest(&mut self) -> String {
        if let Some(ref opt) = self.spec.options {
            sha256::digest(serde_json::to_string(opt).unwrap())
        } else {
            sha256::digest("")
        }
    }

    pub fn get_tfstate(&self) -> Result<Option<String>> {
        if let Some(status) = self.status.clone() {
            if let Some(tf) = status.tfstate {
                let decoded = base64_gz_decode(tf)?;
                Ok(Some(decoded))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    pub fn get_rhaistate(&self) -> Result<Option<String>> {
        if let Some(status) = self.status.clone() {
            if let Some(tf) = status.rhaistate {
                let decoded = base64_gz_decode(tf)?;
                Ok(Some(decoded))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    pub async fn get_tenant_name(&self) -> Result<String> {
        let my_ns = self.metadata.namespace.clone().unwrap();
        let ns_api: Api<Namespace> = Api::all(get_client());
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
        let ns_api: Api<Namespace> = Api::all(get_client());
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

    fn have_condition(&self, cond: &ApplicationCondition) -> bool {
        if let Some(status) = self.status.clone() {
            status.conditions.clone().into_iter().any(|c| {
                c.condition_type == cond.condition_type
                    && c.generation == cond.generation
                    && c.status == cond.status
                    && c.message == cond.message
            })
        } else {
            false
        }
    }

    fn get_conditions_excluding(&self, exclude: Vec<ConditionsType>) -> Vec<ApplicationCondition> {
        let mut ret = Vec::new();
        if let Some(status) = self.status.clone() {
            for c in status.conditions {
                if !exclude.clone().into_iter().any(|exc| c.condition_type == exc) {
                    ret.push(c);
                }
            }
        }
        ret
    }

    async fn patch_status(&mut self, client: Client, patch: serde_json::Value) -> Result<Self> {
        let api = Api::<Self>::namespaced(client.clone(), &self.namespace().unwrap());
        let name = self.metadata.name.clone().unwrap();
        let new_status: Patch<serde_json::Value> = Patch::Merge(json!({
            "apiVersion": "vynil.solidite.fr/v1",
            "kind": "TenantInstance",
            "status": patch
        }));
        let ps = PatchParams::apply(get_short_name().as_str());
        api.patch_status(&name, &ps, &new_status)
            .await
            .map_err(Error::KubeError)
    }

    async fn send_event(&mut self, client: Client, ev: Event) -> Result<()> {
        let recorder = Recorder::new(client.clone(), get_reporter(), self.object_ref(&()));
        recorder.publish(ev).await.map_err(Error::KubeError)
    }

    pub async fn set_status_ready(&mut self, tag: String) -> Result<Self> {
        let client = get_client();
        let generation = self.metadata.generation.unwrap_or(1);
        let mut conditions: Vec<ApplicationCondition> = self.get_conditions_excluding(vec![
            ConditionsType::AgentStarted,
            ConditionsType::Ready,
            ConditionsType::Installed,
            ConditionsType::InitFrom,
            ConditionsType::ScheduleBackup,
        ]);
        conditions.push(ApplicationCondition::ready_ok(generation));
        conditions.push(ApplicationCondition::installed_ok(generation));
        let result = self
            .patch_status(
                client.clone(),
                json!({
                    "conditions": conditions,
                    "tag": tag,
                    "digest": self.clone().get_options_digest()
                }),
            )
            .await?;
        self.send_event(client, Event {
            type_: EventType::Normal,
            reason: "InstallSucceed".to_string(),
            note: None,
            action: "Install".to_string(),
            secondary: None,
        })
        .await?;
        Ok(result)
    }

    pub async fn set_tfstate(&mut self, tfstate: String) -> Result<Self> {
        let client = get_client();
        let generation = self.metadata.generation.unwrap_or(1);
        let encoded = encode_base64_gz(tfstate)?;
        let mut conditions: Vec<ApplicationCondition> =
            self.get_conditions_excluding(vec![ConditionsType::TofuInstalled]);
        conditions.push(ApplicationCondition::tofu_ok(generation));
        let result = self
            .patch_status(
                client.clone(),
                json!({
                    "conditions": conditions,
                    "tfstate": encoded
                }),
            )
            .await?;
        self.send_event(client, Event {
            type_: EventType::Normal,
            reason: "TofuApplySucceed".to_string(),
            note: None,
            action: "TofuApply".to_string(),
            secondary: None,
        })
        .await?;
        Ok(result)
    }

    pub async fn set_status_tofu_failed(&mut self, tfstate: String, reason: String) -> Result<Self> {
        let client = get_client();
        let generation = self.metadata.generation.unwrap_or(1);
        let encoded = encode_base64_gz(tfstate)?;
        let mut conditions: Vec<ApplicationCondition> = self.get_conditions_excluding(vec![
            ConditionsType::AgentStarted,
            ConditionsType::TofuInstalled,
            ConditionsType::Installed,
        ]);
        conditions.push(ApplicationCondition::tofu_ko(&reason, generation));
        conditions.push(ApplicationCondition::installed_ko(&reason, generation));
        if !conditions
            .clone()
            .into_iter()
            .any(|c| c.condition_type == ConditionsType::Ready)
        {
            conditions.push(ApplicationCondition::ready_ko(generation));
        }
        let result = self
            .patch_status(
                client.clone(),
                json!({
                    "conditions": conditions,
                    "tfstate": encoded
                }),
            )
            .await?;
        let mut note = reason;
        note.truncate(1023);
        self.send_event(client, Event {
            type_: EventType::Warning,
            reason: "TofuApplyFailed".to_string(),
            note: Some(note),
            action: "TofuApply".to_string(),
            secondary: None,
        })
        .await?;
        Ok(result)
    }

    pub async fn set_rhaistate(&mut self, rhaistate: String) -> Result<Self> {
        let client = get_client();
        let generation = self.metadata.generation.unwrap_or(1);
        let encoded = encode_base64_gz(rhaistate)?;
        let mut conditions: Vec<ApplicationCondition> =
            self.get_conditions_excluding(vec![ConditionsType::RhaiApplied]);
        conditions.push(ApplicationCondition::rhai_ok(generation));
        let result = self
            .patch_status(
                client.clone(),
                json!({
                    "conditions": conditions,
                    "rhaistate": encoded
                }),
            )
            .await?;
        self.send_event(client, Event {
            type_: EventType::Normal,
            reason: "RhaiApplySucceed".to_string(),
            note: None,
            action: "RhaiApply".to_string(),
            secondary: None,
        })
        .await?;
        Ok(result)
    }

    pub async fn set_status_rhai_failed(&mut self, rhaistate: String, reason: String) -> Result<Self> {
        let client = get_client();
        let generation = self.metadata.generation.unwrap_or(1);
        let encoded = encode_base64_gz(rhaistate)?;
        let mut conditions: Vec<ApplicationCondition> = self.get_conditions_excluding(vec![
            ConditionsType::AgentStarted,
            ConditionsType::RhaiApplied,
            ConditionsType::Installed,
        ]);
        conditions.push(ApplicationCondition::rhai_ko(&reason, generation));
        conditions.push(ApplicationCondition::installed_ko(&reason, generation));
        if !conditions
            .clone()
            .into_iter()
            .any(|c| c.condition_type == ConditionsType::Ready)
        {
            conditions.push(ApplicationCondition::ready_ko(generation));
        }
        let result: TenantInstance = self
            .patch_status(
                client.clone(),
                json!({
                    "conditions": conditions,
                    "rhaistate": encoded
                }),
            )
            .await?;
        let mut note = reason;
        note.truncate(1023);
        self.send_event(client, Event {
            type_: EventType::Warning,
            reason: "RhaiApplyFailed".to_string(),
            note: Some(note),
            action: "RhaiApply".to_string(),
            secondary: None,
        })
        .await?;
        Ok(result)
    }

    pub async fn set_status_init_failed(&mut self, reason: String) -> Result<Self> {
        let client = get_client();
        let generation = self.metadata.generation.unwrap_or(1);
        let mut conditions: Vec<ApplicationCondition> = self.get_conditions_excluding(vec![
            ConditionsType::AgentStarted,
            ConditionsType::InitFrom,
            ConditionsType::Installed,
        ]);
        conditions.push(ApplicationCondition::init_ko(&reason, generation));
        conditions.push(ApplicationCondition::installed_ko(&reason, generation));
        if !conditions
            .clone()
            .into_iter()
            .any(|c| c.condition_type == ConditionsType::Ready)
        {
            conditions.push(ApplicationCondition::ready_ko(generation));
        }
        let result: TenantInstance = self
            .patch_status(
                client.clone(),
                json!({
                    "conditions": conditions
                }),
            )
            .await?;
        let mut note = reason;
        note.truncate(1023);
        self.send_event(client, Event {
            type_: EventType::Warning,
            reason: "InitFromFail".to_string(),
            note: Some(note),
            action: "InitFrom".to_string(),
            secondary: None,
        })
        .await?;
        Ok(result)
    }

    pub async fn set_status_schedule_backup_failed(&mut self, reason: String) -> Result<Self> {
        let client = get_client();
        let generation = self.metadata.generation.unwrap_or(1);
        let mut conditions: Vec<ApplicationCondition> = self.get_conditions_excluding(vec![
            ConditionsType::AgentStarted,
            ConditionsType::ScheduleBackup,
            ConditionsType::Installed,
        ]);
        conditions.push(ApplicationCondition::schedule_ko(&reason, generation));
        conditions.push(ApplicationCondition::installed_ko(&reason, generation));
        if !conditions
            .clone()
            .into_iter()
            .any(|c| c.condition_type == ConditionsType::Ready)
        {
            conditions.push(ApplicationCondition::ready_ko(generation));
        }
        let result: TenantInstance = self
            .patch_status(
                client.clone(),
                json!({
                    "conditions": conditions
                }),
            )
            .await?;
        let mut note = reason;
        note.truncate(1023);
        self.send_event(client, Event {
            type_: EventType::Warning,
            reason: "ScheduleBackupFailed".to_string(),
            note: Some(note),
            action: "ScheduleBackup".to_string(),
            secondary: None,
        })
        .await?;
        Ok(result)
    }

    pub async fn set_status_vitals(&mut self, vitals: Vec<Children>) -> Result<Self> {
        let count = vitals.len();
        let client = get_client();
        let generation = self.metadata.generation.unwrap_or(1);
        let mut conditions: Vec<ApplicationCondition> =
            self.get_conditions_excluding(vec![ConditionsType::VitalApplied]);
        conditions.push(ApplicationCondition::vital_ok(generation));
        let result: TenantInstance = self
            .patch_status(
                client.clone(),
                json!({
                    "conditions": conditions,
                    "vitals": vitals
                }),
            )
            .await?;
        self.send_event(client, Event {
            type_: EventType::Normal,
            reason: "VitalApplySucceed".to_string(),
            note: Some(format!("Applied {} Objects", count)),
            action: "VitalApply".to_string(),
            secondary: None,
        })
        .await?;
        Ok(result)
    }

    pub async fn set_status_vital_failed(&mut self, reason: String) -> Result<Self> {
        let client = get_client();
        let generation = self.metadata.generation.unwrap_or(1);
        let mut conditions: Vec<ApplicationCondition> = self.get_conditions_excluding(vec![
            ConditionsType::AgentStarted,
            ConditionsType::VitalApplied,
            ConditionsType::Installed,
        ]);
        conditions.push(ApplicationCondition::vital_ko(&reason, generation));
        conditions.push(ApplicationCondition::installed_ko(&reason, generation));
        if !conditions
            .clone()
            .into_iter()
            .any(|c| c.condition_type == ConditionsType::Ready)
        {
            conditions.push(ApplicationCondition::ready_ko(generation));
        }
        let result: TenantInstance = self
            .patch_status(
                client.clone(),
                json!({
                    "conditions": conditions,
                }),
            )
            .await?;
        let mut note = reason;
        note.truncate(1023);
        self.send_event(client, Event {
            type_: EventType::Warning,
            reason: "VitalApplyFailed".to_string(),
            note: Some(note),
            action: "VitalApply".to_string(),
            secondary: None,
        })
        .await?;
        Ok(result)
    }

    pub async fn set_status_scalables(&mut self, scalables: Vec<Children>) -> Result<Self> {
        let count = scalables.len();
        let client = get_client();
        let generation = self.metadata.generation.unwrap_or(1);
        let mut conditions: Vec<ApplicationCondition> =
            self.get_conditions_excluding(vec![ConditionsType::ScalableApplied]);
        conditions.push(ApplicationCondition::scalable_ok(generation));
        let result: TenantInstance = self
            .patch_status(
                client.clone(),
                json!({
                    "conditions": conditions,
                    "scalables": scalables
                }),
            )
            .await?;
        self.send_event(client, Event {
            type_: EventType::Normal,
            reason: "ScalableApplySucceed".to_string(),
            note: Some(format!("Applied {} Objects", count)),
            action: "ScalableApply".to_string(),
            secondary: None,
        })
        .await?;
        Ok(result)
    }

    pub async fn set_status_scalable_failed(&mut self, reason: String) -> Result<Self> {
        let client = get_client();
        let generation = self.metadata.generation.unwrap_or(1);
        let mut conditions: Vec<ApplicationCondition> = self.get_conditions_excluding(vec![
            ConditionsType::AgentStarted,
            ConditionsType::ScalableApplied,
            ConditionsType::Installed,
        ]);
        conditions.push(ApplicationCondition::scalable_ko(&reason, generation));
        conditions.push(ApplicationCondition::installed_ko(&reason, generation));
        if !conditions
            .clone()
            .into_iter()
            .any(|c| c.condition_type == ConditionsType::Ready)
        {
            conditions.push(ApplicationCondition::ready_ko(generation));
        }
        let result: TenantInstance = self
            .patch_status(
                client.clone(),
                json!({
                    "conditions": conditions,
                }),
            )
            .await?;
        let mut note = reason;
        note.truncate(1023);
        self.send_event(client, Event {
            type_: EventType::Warning,
            reason: "ScalableApplyFailed".to_string(),
            note: Some(note),
            action: "ScalableApply".to_string(),
            secondary: None,
        })
        .await?;
        Ok(result)
    }

    pub async fn set_status_others(&mut self, others: Vec<Children>) -> Result<Self> {
        let count = others.len();
        let client = get_client();
        let generation = self.metadata.generation.unwrap_or(1);
        let mut conditions: Vec<ApplicationCondition> =
            self.get_conditions_excluding(vec![ConditionsType::OtherApplied]);
        conditions.push(ApplicationCondition::other_ok(generation));
        let result: TenantInstance = self
            .patch_status(
                client.clone(),
                json!({
                    "conditions": conditions,
                    "others": others
                }),
            )
            .await?;
        self.send_event(client, Event {
            type_: EventType::Normal,
            reason: "OtherApplySucceed".to_string(),
            note: Some(format!("Applied {} Objects", count)),
            action: "OtherApply".to_string(),
            secondary: None,
        })
        .await?;
        Ok(result)
    }

    pub async fn set_status_other_failed(&mut self, reason: String) -> Result<Self> {
        let client = get_client();
        let generation = self.metadata.generation.unwrap_or(1);
        let mut conditions: Vec<ApplicationCondition> = self.get_conditions_excluding(vec![
            ConditionsType::AgentStarted,
            ConditionsType::OtherApplied,
            ConditionsType::Installed,
        ]);
        conditions.push(ApplicationCondition::other_ko(&reason, generation));
        conditions.push(ApplicationCondition::installed_ko(&reason, generation));
        if !conditions
            .clone()
            .into_iter()
            .any(|c| c.condition_type == ConditionsType::Ready)
        {
            conditions.push(ApplicationCondition::ready_ko(generation));
        }
        let result: TenantInstance = self
            .patch_status(
                client.clone(),
                json!({
                    "conditions": conditions,
                }),
            )
            .await?;
        let mut note = reason;
        note.truncate(1023);
        self.send_event(client, Event {
            type_: EventType::Warning,
            reason: "OtherApplyFailed".to_string(),
            note: Some(note),
            action: "OtherApply".to_string(),
            secondary: None,
        })
        .await?;
        Ok(result)
    }

    pub async fn set_agent_started(&mut self) -> Result<Self> {
        let client = get_client();
        let generation = self.metadata.generation.unwrap_or(1);
        let cond = ApplicationCondition::agent_started(generation);
        if !self.have_condition(&cond) {
            let mut conditions: Vec<ApplicationCondition> =
                self.get_conditions_excluding(vec![ConditionsType::AgentStarted]);
            conditions.push(cond);
            let result = self
                .patch_status(
                    client.clone(),
                    json!({
                        "conditions": conditions,
                    }),
                )
                .await?;
            self.send_event(client, Event {
                type_: EventType::Normal,
                reason: "AgentStarted".to_string(),
                note: None,
                action: "AgentStart".to_string(),
                secondary: None,
            })
            .await?;
            Ok(result)
        } else {
            Ok(self.clone())
        }
    }

    pub async fn set_missing_box(&mut self, jukebox: String) -> Result<Self> {
        let client = get_client();
        let generation = self.metadata.generation.unwrap_or(1);
        let cond = ApplicationCondition::missing_box(&jukebox, generation);
        if !self.have_condition(&cond) {
            let mut conditions: Vec<ApplicationCondition> =
                self.get_conditions_excluding(vec![ConditionsType::AgentStarted]);
            conditions.push(cond);
            let result = self
                .patch_status(
                    client.clone(),
                    json!({
                        "conditions": conditions,
                    }),
                )
                .await?;
            self.send_event(client, Event {
                type_: EventType::Warning,
                reason: "MissingJukebox".to_string(),
                note: Some(format!("JukeBox {jukebox} doesnt exist")),
                action: "AgentStart".to_string(),
                secondary: None,
            })
            .await?;
            Ok(result)
        } else {
            Ok(self.clone())
        }
    }

    pub async fn set_missing_package(&mut self, category: String, package: String) -> Result<Self> {
        let client = get_client();
        let generation = self.metadata.generation.unwrap_or(1);
        let cond = ApplicationCondition::missing_package(&category, &package, generation);
        if !self.have_condition(&cond) {
            let mut conditions: Vec<ApplicationCondition> =
                self.get_conditions_excluding(vec![ConditionsType::AgentStarted]);
            conditions.push(cond);
            let result = self
                .patch_status(
                    client.clone(),
                    json!({
                        "conditions": conditions,
                    }),
                )
                .await?;
            self.send_event(client, Event {
                type_: EventType::Warning,
                reason: "MissingPackage".to_string(),
                note: Some(format!("Package {category}/{package} doesnt exist")),
                action: "AgentStart".to_string(),
                secondary: None,
            })
            .await?;
            Ok(result)
        } else {
            Ok(self.clone())
        }
    }

    pub async fn set_missing_requirement(&mut self, reason: String) -> Result<Self> {
        let client = get_client();
        let generation = self.metadata.generation.unwrap_or(1);
        let cond = ApplicationCondition::missing_requirement(&reason, generation);
        if !self.have_condition(&cond) {
            let mut conditions: Vec<ApplicationCondition> =
                self.get_conditions_excluding(vec![ConditionsType::AgentStarted]);
            conditions.push(cond);
            let result = self
                .patch_status(
                    client.clone(),
                    json!({
                        "conditions": conditions,
                    }),
                )
                .await?;
            let mut note = reason;
            note.truncate(1023);
            self.send_event(client, Event {
                type_: EventType::Warning,
                reason: "MissingRequirement".to_string(),
                note: Some(note),
                action: "AgentStart".to_string(),
                secondary: None,
            })
            .await?;
            Ok(result)
        } else {
            Ok(self.clone())
        }
    }

    pub fn rhai_get(namespace: String, name: String) -> RhaiRes<Self> {
        block_in_place(|| Handle::current().block_on(async move { Self::get(namespace, name).await }))
            .map_err(rhai_err)
    }

    pub fn rhai_list(namespace: String) -> RhaiRes<Vec<Self>> {
        block_in_place(|| Handle::current().block_on(async move { Self::list(namespace).await }))
            .map_err(rhai_err)
            .map(|lst| lst.into_iter().collect())
    }

    pub fn get_metadata(&mut self) -> RhaiRes<Dynamic> {
        let v = serde_json::to_string(&self.metadata).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }

    pub fn get_spec(&mut self) -> RhaiRes<Dynamic> {
        let v = serde_json::to_string(&self.spec).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }

    pub fn get_status(&mut self) -> RhaiRes<Dynamic> {
        let v = serde_json::to_string(&self.status).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }

    pub fn rhai_get_tfstate(&mut self) -> RhaiRes<String> {
        let res = self.get_tfstate().map_err(rhai_err)?;
        if res.is_some() {
            Ok(res.unwrap())
        } else {
            Ok("".to_string())
        }
    }

    pub fn rhai_get_rhaistate(&mut self) -> RhaiRes<String> {
        let res = self.get_rhaistate().map_err(rhai_err)?;
        if res.is_some() {
            Ok(res.unwrap())
        } else {
            Ok("".to_string())
        }
    }

    pub fn rhai_set_status_ready(&mut self, tag: String) -> RhaiRes<Self> {
        block_in_place(|| Handle::current().block_on(async move { self.set_status_ready(tag).await }))
            .map_err(rhai_err)
    }

    pub fn rhai_set_tfstate(&mut self, tfstate: String) -> RhaiRes<Self> {
        block_in_place(|| Handle::current().block_on(async move { self.set_tfstate(tfstate).await }))
            .map_err(rhai_err)
    }

    pub fn rhai_set_status_tofu_failed(&mut self, tfstate: String, reason: String) -> RhaiRes<Self> {
        block_in_place(|| {
            Handle::current().block_on(async move { self.set_status_tofu_failed(tfstate, reason).await })
        })
        .map_err(rhai_err)
    }

    pub fn rhai_set_status_schedule_backup_failed(&mut self, reason: String) -> RhaiRes<Self> {
        block_in_place(|| {
            Handle::current().block_on(async move { self.set_status_schedule_backup_failed(reason).await })
        })
        .map_err(rhai_err)
    }

    pub fn rhai_set_status_init_failed(&mut self, reason: String) -> RhaiRes<Self> {
        block_in_place(|| {
            Handle::current().block_on(async move { self.set_status_init_failed(reason).await })
        })
        .map_err(rhai_err)
    }

    pub fn rhai_set_rhaistate(&mut self, rhaistate: String) -> RhaiRes<Self> {
        block_in_place(|| Handle::current().block_on(async move { self.set_rhaistate(rhaistate).await }))
            .map_err(rhai_err)
    }

    pub fn rhai_set_status_rhai_failed(&mut self, rhaistate: String, reason: String) -> RhaiRes<Self> {
        block_in_place(|| {
            Handle::current().block_on(async move { self.set_status_rhai_failed(rhaistate, reason).await })
        })
        .map_err(rhai_err)
    }

    pub fn rhai_set_status_vitals(&mut self, list: Dynamic) -> RhaiRes<Self> {
        block_in_place(|| {
            Handle::current().block_on(async move {
                let v = serde_json::to_string(&list).map_err(Error::SerializationError)?;
                let lst = serde_json::from_str(&v).map_err(Error::SerializationError)?;
                self.set_status_vitals(lst).await
            })
        })
        .map_err(rhai_err)
    }

    pub fn rhai_set_status_vital_failed(&mut self, reason: String) -> RhaiRes<Self> {
        block_in_place(|| {
            Handle::current().block_on(async move { self.set_status_vital_failed(reason).await })
        })
        .map_err(rhai_err)
    }

    pub fn rhai_set_status_scalables(&mut self, list: Dynamic) -> RhaiRes<Self> {
        block_in_place(|| {
            Handle::current().block_on(async move {
                let v = serde_json::to_string(&list).map_err(Error::SerializationError)?;
                let lst = serde_json::from_str(&v).map_err(Error::SerializationError)?;
                self.set_status_scalables(lst).await
            })
        })
        .map_err(rhai_err)
    }

    pub fn rhai_set_status_scalable_failed(&mut self, reason: String) -> RhaiRes<Self> {
        block_in_place(|| {
            Handle::current().block_on(async move { self.set_status_scalable_failed(reason).await })
        })
        .map_err(rhai_err)
    }

    pub fn rhai_set_status_others(&mut self, list: Dynamic) -> RhaiRes<Self> {
        block_in_place(|| {
            Handle::current().block_on(async move {
                let v = serde_json::to_string(&list).map_err(Error::SerializationError)?;
                let lst = serde_json::from_str(&v).map_err(Error::SerializationError)?;
                self.set_status_others(lst).await
            })
        })
        .map_err(rhai_err)
    }

    pub fn rhai_set_status_other_failed(&mut self, reason: String) -> RhaiRes<Self> {
        block_in_place(|| {
            Handle::current().block_on(async move { self.set_status_other_failed(reason).await })
        })
        .map_err(rhai_err)
    }

    pub fn rhai_set_agent_started(&mut self) -> RhaiRes<Self> {
        block_in_place(|| Handle::current().block_on(async move { self.set_agent_started().await }))
            .map_err(rhai_err)
    }

    pub fn rhai_set_missing_box(&mut self, jukebox: String) -> RhaiRes<Self> {
        block_in_place(|| Handle::current().block_on(async move { self.set_missing_box(jukebox).await }))
            .map_err(rhai_err)
    }

    pub fn rhai_set_missing_package(&mut self, category: String, package: String) -> RhaiRes<Self> {
        block_in_place(|| {
            Handle::current().block_on(async move { self.set_missing_package(category, package).await })
        })
        .map_err(rhai_err)
    }

    pub fn rhai_set_missing_requirement(&mut self, reason: String) -> RhaiRes<Self> {
        block_in_place(|| {
            Handle::current().block_on(async move { self.set_missing_requirement(reason).await })
        })
        .map_err(rhai_err)
    }

    pub fn rhai_get_tenant_name(&mut self) -> RhaiRes<String> {
        block_in_place(|| Handle::current().block_on(async move { self.get_tenant_name().await }))
            .map_err(rhai_err)
    }

    pub fn rhai_get_tenant_namespaces(&mut self) -> RhaiRes<Dynamic> {
        block_in_place(|| {
            Handle::current().block_on(async move {
                let arr = self.get_tenant_namespaces().await;
                if arr.is_ok() {
                    let arr = arr.unwrap();
                    let v = serde_json::to_string(&arr).map_err(Error::SerializationError)?;
                    serde_json::from_str::<Dynamic>(&v).map_err(Error::SerializationError)
                } else {
                    arr.map(|_| Dynamic::from(""))
                }
            })
        })
        .map_err(rhai_err)
    }
}
