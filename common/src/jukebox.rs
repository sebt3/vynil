use crate::{
    context::{get_client, get_reporter, get_short_name},
    rhai_err,
    vynilpackage::VynilPackage,
    Error, Result, RhaiRes,
};
use chrono::{DateTime, Utc};
use kube::{
    api::{Api, ListParams, ObjectList, Patch, PatchParams},
    runtime::events::{Event, EventType, Recorder},
    Client, CustomResource, Resource,
};
use rhai::Dynamic;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::{runtime::Handle, task::block_in_place};

/// JukeBox Source type
#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JukeBoxDef {
    /// List of oci images
    List(Vec<String>),
    /// Harbor project to list image from
    Harbor { registry: String, project: String },
    /// Custom script that produce the image list
    Script(String),
}
impl Default for JukeBoxDef {
    fn default() -> Self {
        Self::List(["docker.io/sebt3/vynil".to_string()].into())
    }
}

/// JukeBox Maturity
#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum JukeBoxMaturity {
    #[default]
    Stable,
    Beta,
    Alpha,
}

/// Describe a source of vynil packages jukebox
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    kind = "JukeBox",
    status = "JukeBoxStatus",
    shortname = "box",
    group = "vynil.solidite.fr",
    version = "v1"
)]
#[kube(
    doc = "Custom resource representing a JukeBox, source of vynil packages",
    printcolumn = r#"
    {"name":"schedule", "type":"string", "description":"Update schedule", "jsonPath":".spec.schedule"},
    {"name":"last_updated", "type":"date", "description":"Last update date", "format": "date-time", "jsonPath":".status.conditions[?(@.type == 'Ready')].lastTransitionTime"},
    {"name":"message", "type":"string", "description":"Message", "jsonPath":".status.conditions[?(@.type == 'Updated')].message"}"#
)]
pub struct JukeBoxSpec {
    /// Source type
    pub source: Option<JukeBoxDef>,
    /// Jukebox maturity (stable/beta/alpha)
    pub maturity: Option<JukeBoxMaturity>,
    /// ImagePullSecret name in the vynil-system namespace
    pub pull_secret: Option<String>,
    /// Actual cron-type expression that defines the interval of the updates.
    pub schedule: String,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, JsonSchema, Default)]
pub enum ConditionsType {
    #[default]
    Ready,
    Updated,
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
            "Updated succesfully",
            ConditionsStatus::True,
            ConditionsType::Ready,
            generation,
        )
    }

    pub fn ready_ko(generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            "No successful update",
            ConditionsStatus::False,
            ConditionsType::Ready,
            generation,
        )
    }

    pub fn updated_ko(message: &str, generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            message,
            ConditionsStatus::False,
            ConditionsType::Updated,
            generation,
        )
    }

    pub fn updated_ok(generation: i64) -> ApplicationCondition {
        ApplicationCondition::new(
            "Updated succesfully",
            ConditionsStatus::True,
            ConditionsType::Updated,
            generation,
        )
    }
}

/// The status object of `JukeBox`
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct JukeBoxStatus {
    /// JukeBox Conditions
    pub conditions: Vec<ApplicationCondition>,
    /// Vynil packages for this box
    pub packages: Vec<VynilPackage>,
}

impl JukeBox {
    pub async fn get(name: String) -> Result<Self> {
        let api = Api::<Self>::all(get_client());
        api.get(&name).await.map_err(Error::KubeError)
    }

    pub async fn list() -> Result<ObjectList<Self>> {
        let api = Api::<Self>::all(get_client());
        let lp = ListParams::default();
        api.list(&lp).await.map_err(Error::KubeError)
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
        let api = Api::<Self>::all(client.clone());
        let name = self.metadata.name.clone().unwrap();
        let new_status: Patch<serde_json::Value> = Patch::Merge(json!({
            "apiVersion": "vynil.solidite.fr/v1",
            "kind": "JukeBox",
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

    pub async fn set_status_updated(&mut self, packages: Vec<VynilPackage>) -> Result<Self> {
        let count = packages.len();
        let client = get_client();
        let generation = self.metadata.generation.unwrap_or(1);
        let conditions: Vec<ApplicationCondition> = vec![
            ApplicationCondition::updated_ok(generation),
            ApplicationCondition::ready_ok(generation),
        ];
        let result = self
            .patch_status(
                client.clone(),
                json!({
                    "conditions": conditions,
                    "packages": packages,
                }),
            )
            .await?;
        self.send_event(client, Event {
            type_: EventType::Normal,
            reason: "ScanSucceed".to_string(),
            note: Some(format!("Found {} packages", count)),
            action: "Scan".to_string(),
            secondary: None,
        })
        .await?;
        Ok(result)
    }

    pub async fn set_status_failed(&mut self, reason: String) -> Result<Self> {
        let client = get_client();
        let generation = self.metadata.generation.unwrap_or(1);
        let mut conditions: Vec<ApplicationCondition> =
            self.get_conditions_excluding(vec![ConditionsType::Updated]);
        conditions.push(ApplicationCondition::updated_ko(&reason, generation));
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
                }),
            )
            .await?;
        self.send_event(client, Event {
            type_: EventType::Warning,
            reason: "ScanFailed".to_string(),
            note: Some(reason),
            action: "Scan".to_string(),
            secondary: None,
        })
        .await?;
        Ok(result)
    }

    pub fn rhai_get(name: String) -> RhaiRes<Self> {
        block_in_place(|| Handle::current().block_on(async move { Self::get(name).await })).map_err(rhai_err)
    }

    pub fn rhai_list() -> RhaiRes<Vec<Self>> {
        block_in_place(|| Handle::current().block_on(async move { Self::list().await }))
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

    pub fn rhai_set_status_updated(&mut self, list: Dynamic) -> RhaiRes<Self> {
        block_in_place(|| {
            Handle::current().block_on(async move {
                let v = serde_json::to_string(&list).map_err(Error::SerializationError)?;
                let lst = serde_json::from_str(&v).map_err(Error::SerializationError)?;
                self.set_status_updated(lst).await
            })
        })
        .map_err(rhai_err)
    }

    pub fn rhai_set_status_failed(&mut self, reason: String) -> RhaiRes<JukeBox> {
        block_in_place(|| Handle::current().block_on(async move { self.set_status_failed(reason).await }))
            .map_err(rhai_err)
    }
}
