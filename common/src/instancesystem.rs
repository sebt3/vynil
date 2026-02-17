use chrono::{DateTime, Utc};
use kube::{
    CustomResource, Resource, ResourceExt,
    runtime::events::{Event, EventType},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Describe a source of vynil packages jukebox
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    kind = "SystemInstance",
    status = "SystemInstanceStatus",
    shortname = "vsi",
    group = "vynil.solidite.fr",
    version = "v1",
    namespaced
)]
#[kube(
    doc = "Custom resource representing an Vynil cluster package installation",
    printcolumn = r#"
    {"name":"Juke",   "type":"string", "description":"JukeBox", "jsonPath":".spec.jukebox"},
    {"name":"cat",    "type":"string", "description":"Category", "jsonPath":".spec.category"},
    {"name":"pkg",    "type":"string", "description":"Package", "jsonPath":".spec.package"},
    {"name":"tag",    "type":"string", "description":"Version", "jsonPath":".status.tag"},
    {"name":"last_updated", "type":"date", "description":"Last update date", "format": "date-time", "jsonPath":".status.conditions[?(@.type == 'Ready')].lastTransitionTime"},
    {"name":"stage",  "type":"string", "description":"Stage", "jsonPath":".status.conditions[-1:].type"},
    {"name":"errors", "type":"string", "description":"Errors", "jsonPath":".status.conditions[?(@.status == 'False')].message"}"#
)]
pub struct SystemInstanceSpec {
    /// The jukebox source name
    pub jukebox: String,
    /// The category name
    pub category: String,
    /// The package name
    pub package: String,
    /// The package version
    pub version: Option<String>,
    /// Parameters
    pub options: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, JsonSchema, Default)]
pub enum ConditionsType {
    #[default]
    Ready,
    Installed,
    AgentStarted,
    CrdApplied,
    TofuInstalled,
    SystemApplied,
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

impl_condition_common!();
impl_condition_crds!();

impl ApplicationCondition {
    pub fn system_ko(message: &str, generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            message,
            ConditionsStatus::False,
            ConditionsType::SystemApplied,
            generation,
        )
    }

    pub fn system_ok(generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            "Templates applied succesfully",
            ConditionsStatus::True,
            ConditionsType::SystemApplied,
            generation,
        )
    }
}

/// The status object of `SystemInstance`
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct SystemInstanceStatus {
    /// SystemInstance Conditions
    pub conditions: Vec<ApplicationCondition>,
    /// Current tag
    pub tag: Option<String>,
    /// Options digests
    pub digest: Option<String>,
    /// Current terraform status (gzip+base64)
    pub tfstate: Option<String>,
    /// Current rhai status (gzip+base64) (for custom package information)
    pub rhaistate: Option<String>,
    /// List of system children
    pub systems: Option<Vec<crate::Children>>,
    /// List of crds children
    pub crds: Option<Vec<String>>,
}

impl SystemInstance {
    pub fn have_child(&self) -> bool {
        if let Some(status) = self.status.clone() {
            if status.rhaistate.is_some() {
                return true;
            }
            if status.tfstate.is_some() {
                return true;
            }
            if let Some(child) = status.systems.clone() {
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

    pub async fn set_status_ready(&mut self, tag: String) -> crate::Result<Self> {
        let client = crate::context::get_client_async().await;
        let generation = self.metadata.generation.unwrap_or(1);
        let mut conditions: Vec<ApplicationCondition> = self.get_conditions_excluding(vec![
            ConditionsType::AgentStarted,
            ConditionsType::Ready,
            ConditionsType::Installed,
        ]);
        conditions.push(ApplicationCondition::ready_ok(generation));
        conditions.push(ApplicationCondition::installed_ok(generation));
        let result = self
            .patch_status(
                client.clone(),
                serde_json::json!({
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

    pub async fn set_status_systems(
        &mut self,
        systems: Vec<crate::Children>,
    ) -> crate::Result<Self> {
        let count = systems.len();
        let client = crate::context::get_client_async().await;
        let generation = self.metadata.generation.unwrap_or(1);
        let mut conditions: Vec<ApplicationCondition> =
            self.get_conditions_excluding(vec![ConditionsType::SystemApplied]);
        conditions.push(ApplicationCondition::system_ok(generation));
        let result = self
            .patch_status(
                client.clone(),
                serde_json::json!({ "conditions": conditions, "systems": systems }),
            )
            .await?;
        self.send_event(client, Event {
            type_: EventType::Normal,
            reason: "SystemApplySucceed".to_string(),
            note: Some(format!("Applied {} Objects", count)),
            action: "SystemApply".to_string(),
            secondary: None,
        })
        .await?;
        Ok(result)
    }

    pub async fn set_status_system_failed(&mut self, reason: String) -> crate::Result<Self> {
        let client = crate::context::get_client_async().await;
        let generation: i64 = self.metadata.generation.unwrap_or(1);
        let mut conditions: Vec<ApplicationCondition> = self.get_conditions_excluding(vec![
            ConditionsType::AgentStarted,
            ConditionsType::SystemApplied,
            ConditionsType::Installed,
        ]);
        conditions.push(ApplicationCondition::system_ko(&reason, generation));
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
                serde_json::json!({ "conditions": conditions }),
            )
            .await?;
        let mut note = reason;
        note.truncate(1023);
        self.send_event(client, Event {
            type_: EventType::Warning,
            reason: "SystemApplyFailed".to_string(),
            note: Some(note),
            action: "SystemApply".to_string(),
            secondary: None,
        })
        .await?;
        Ok(result)
    }

    pub fn rhai_set_status_systems(&mut self, list: rhai::Dynamic) -> crate::RhaiRes<Self> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let v = serde_json::to_string(&list)
                    .map_err(crate::Error::SerializationError)?;
                let lst = serde_json::from_str(&v).map_err(crate::Error::SerializationError)?;
                self.set_status_systems(lst).await
            })
        })
        .map_err(crate::rhai_err)
    }

    pub fn rhai_set_status_system_failed(&mut self, reason: String) -> crate::RhaiRes<Self> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async move { self.set_status_system_failed(reason).await })
        })
        .map_err(crate::rhai_err)
    }
}

impl_instance_common!(SystemInstance, "SystemInstance");
impl_instance_crds!(SystemInstance);
