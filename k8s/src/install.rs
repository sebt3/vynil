use kube::{api::{Api, Patch, PatchParams}, CustomResource, Client};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use serde_json::json;

/// Generate the Kubernetes wrapper struct `Install` from our Spec and Status struct
///
/// This provides a hook for generating the CRD yaml (in crdgen.rs)
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(kind = "Install", group = "vynil.solidite.fr", version = "v1", namespaced)]
#[kube(status = "InstallStatus", shortname = "inst", printcolumn = r#"
{"name":"dist", "type":"string", "description":"Distribution", "jsonPath":".spec.distrib"},
{"name":"cat",  "type":"string", "description":"Category", "jsonPath":".spec.category"},
{"name":"app",  "type":"string", "description":"Component", "jsonPath":".spec.component"},
{"name":"last_updated", "type":"string", "description":"Last update date", "format": "date-time", "jsonPath":".status.last_updated"}"#)]
/// Maybe
pub struct InstallSpec {
    /// The distribution source name
    pub distrib: String,
    /// The category name
    pub category: String,
    /// The package name
    pub component: String,
    /// Parameters
    pub options: Option<serde_json::Map<String, serde_json::Value>>,
    /// Actual cron-type expression that defines the interval of the upgrades.
    pub schedule: Option<String>,
    /// Should we plan
    pub plan: Option<bool>,
}
/// The status object of `Install`
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct InstallStatus {
    /// Set with the messages if any error occured
    pub errors: Option<Vec<String>>,
    /// Currently planned changed, only set if planned is true
    pub plan: Option<serde_json::Map<String, serde_json::Value>>,
    /// Current terraform status
    pub tfstate: Option<serde_json::Map<String, serde_json::Value>>,
    /// Last update date
    pub last_updated: DateTime<Utc>,
    /// Have we planned the project
    pub planned: bool,
    /// Options digests
    pub digest: String,
}

impl Install {
    pub fn name(&self) -> String {
        if let Some(ref name) = self.metadata.name {
            name.clone()
        } else {
            String::new()
        }
    }
    pub fn options(&self) -> serde_json::Map<std::string::String, serde_json::Value> {
        if let Some(ref opt) = self.spec.options {
            opt.clone()
        } else {
            serde_json::Map::new()
        }
    }
    pub fn options_digest(&self) -> String {
        if let Some(ref opt) = self.spec.options {
            sha256::digest(serde_json::to_string(opt).unwrap())
        } else {
            sha256::digest("")
        }
    }
    pub fn options_status(&self) -> bool {
        if let Some(ref status) = self.status {
            self.options_digest() == status.digest
        } else {
            self.options_digest() == ""
        }
    }
    pub fn last_updated(&self) -> DateTime<Utc> {
        self.status.as_ref().map_or_else(Utc::now, |s| s.last_updated)
    }
    pub fn should_plan(&self) -> bool {
        if let Some(p) = self.spec.plan {p} else {false}
    }
    pub fn was_planned(&self) -> bool {
        self.status.as_ref().map_or(false,|s| s.planned)
    }
    fn current_plan(&self) -> serde_json::Map<String, serde_json::Value> {
        if let Some(ref status) = self.status {
            if let Some(ref plan) = status.plan {
                plan.clone()
            } else { serde_json::Map::new() }
        } else { serde_json::Map::new() }
    }
    pub fn plan(&self) -> serde_json::Map<String, serde_json::Value> {
        self.status.clone().map(|s| s.plan.unwrap_or_default()).unwrap_or_default()
    }
    fn current_tfstate(&self) -> serde_json::Map<String, serde_json::Value> {
        if let Some(ref status) = self.status {
            if let Some(ref tfstate) = status.tfstate {
                tfstate.clone()
            } else { serde_json::Map::new() }
        } else { serde_json::Map::new() }
    }
    pub fn tfstate(&self) -> serde_json::Map<String, serde_json::Value> {
        self.status.clone().map(|s| s.tfstate.unwrap_or_default()).unwrap_or_default()
    }
    pub async fn update_status_errors(&self, client: Client, manager: &str, errors: Vec<String>) {
        let name = self.name();
        let insts: Api<Install> = Api::namespaced(client, self.metadata.namespace.clone().unwrap().as_str());
        let last_updated = self.last_updated();
        let new_status = Patch::Apply(json!({
            "apiVersion": "vynil.solidite.fr/v1",
            "kind": "Install",
            "status": InstallStatus {
                plan: Some(self.current_plan()),
                tfstate: Some(self.current_tfstate()),
                errors: Some(errors),
                planned: self.was_planned(),
                last_updated,
                digest: String::new()
            }
        }));
        let ps = PatchParams::apply(manager).force();
        let _o = insts
            .patch_status(&name, &ps, &new_status)
            .await;
    }
    pub async fn update_status_plan(&self, client: Client, manager: &str, plan: serde_json::Map<String, serde_json::Value>) {
        let name = self.name();
        let insts: Api<Install> = Api::namespaced(client, self.metadata.namespace.clone().unwrap().as_str());
        let last_updated = Utc::now();
        let new_status = Patch::Apply(json!({
            "apiVersion": "vynil.solidite.fr/v1",
            "kind": "Install",
            "status": InstallStatus {
                errors: Some(Vec::new()),
                tfstate: Some(self.current_tfstate()),
                planned: true,
                plan: Some(plan),
                last_updated,
                digest: self.options_digest()
            }
        }));
        let ps = PatchParams::apply(manager).force();
        let _o = insts
            .patch_status(&name, &ps, &new_status)
            .await;
    }
    pub async fn update_status_apply(&self, client: Client, manager: &str, tfstate: serde_json::Map<String, serde_json::Value>) {
        let name = self.name();
        let insts: Api<Install> = Api::namespaced(client, self.metadata.namespace.clone().unwrap().as_str());
        let last_updated = Utc::now();
        let new_status = Patch::Apply(json!({
            "apiVersion": "vynil.solidite.fr/v1",
            "kind": "Install",
            "status": InstallStatus {
                errors: Some(Vec::new()),
                plan: Some(self.current_plan()),
                planned: false,
                tfstate: Some(tfstate),
                last_updated,
                digest: self.options_digest()
            }
        }));
        let ps = PatchParams::apply(manager).force();
        let _o = insts
            .patch_status(&name, &ps, &new_status)
            .await;
    }
}
