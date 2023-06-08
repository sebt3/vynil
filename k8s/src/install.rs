use kube::{api::{Api, Patch, PatchParams}, CustomResource, Client};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use serde_json::json;

pub const STATUS_ERRORS: &str = "errors";
pub const STATUS_INSTALLED: &str = "installed";
pub const STATUS_PLANNED: &str = "planned";
pub const STATUS_INSTALLING: &str = "started";
pub const STATUS_PLANNING: &str = "planning";
pub const STATUS_MISSING_DIST: &str = "missing distribution";
pub const STATUS_MISSING_COMP: &str = "missing component";
pub const STATUS_MISSING_PROV: &str = "missing provider config";
pub const STATUS_MISSING_DEPS: &str = "missing dependencies";
pub const STATUS_WAITING_DEPS: &str = "waiting dependencies";

/// Generate the Kubernetes wrapper struct `Install` from our Spec and Status struct
///
/// This provides a hook for generating the CRD yaml (in crdgen.rs)
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(kind = "Install", group = "vynil.solidite.fr", version = "v1", namespaced)]
#[kube(status = "InstallStatus", shortname = "inst", printcolumn = r#"
{"name":"dist",   "type":"string", "description":"Distribution", "jsonPath":".spec.distrib"},
{"name":"cat",    "type":"string", "description":"Category", "jsonPath":".spec.category"},
{"name":"app",    "type":"string", "description":"Component", "jsonPath":".spec.component"},
{"name":"status", "type":"string", "description":"Status", "jsonPath":".status.status"},
{"name":"errors", "type":"string", "description":"Status", "jsonPath":".status.errors[*]"},
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
    /// Current high-level status of the installation
    pub status: String,
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
    pub fn have_tfstate(&self) -> bool {
        if let Some(ref status) = self.status {
            status.tfstate.is_some()
        } else {false}
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
    async fn update_status_typed(&self, client: Client, manager: &str, errors: Vec<String>, typed: &str) -> Result<Install, kube::Error> {
        let name = self.name();
        let insts: Api<Install> = Api::namespaced(client, self.metadata.namespace.clone().unwrap().as_str());
        let last_updated = self.last_updated();
        let new_status = Patch::Apply(json!({
            "apiVersion": "vynil.solidite.fr/v1",
            "kind": "Install",
            "status": InstallStatus {
                status: typed.to_string(),
                plan: Some(self.current_plan()),
                tfstate: Some(self.current_tfstate()),
                errors: Some(errors),
                planned: self.was_planned(),
                last_updated,
                digest: String::new()
            }
        }));
        let ps = PatchParams::apply(manager).force();
        insts.patch_status(&name, &ps, &new_status).await
    }
    pub async fn update_status_errors(&self, client: Client, manager: &str, errors: Vec<String>) -> Result<Install, kube::Error> {
        self.update_status_typed(client, manager, errors, STATUS_ERRORS).await
    }
    pub async fn update_status_errors_tfstate(&self, client: Client, manager: &str, errors: Vec<String>, tfstate: serde_json::Map<String, serde_json::Value>) -> Result<Install, kube::Error> {
        let name = self.name();
        let insts: Api<Install> = Api::namespaced(client, self.metadata.namespace.clone().unwrap().as_str());
        let last_updated = Utc::now();
        let new_status = Patch::Apply(json!({
            "apiVersion": "vynil.solidite.fr/v1",
            "kind": "Install",
            "status": InstallStatus {
                status: STATUS_ERRORS.to_string(),
                errors: Some(errors),
                plan: Some(self.current_plan()),
                planned: false,
                tfstate: Some(tfstate),
                last_updated,
                digest: self.options_digest()
            }
        }));
        let ps = PatchParams::apply(manager).force();
        insts.patch_status(&name, &ps, &new_status).await
    }
    pub async fn update_status_missing_distrib(&self, client: Client, manager: &str, errors: Vec<String>) -> Result<Install, kube::Error> {
        self.update_status_typed(client, manager, errors, STATUS_MISSING_DIST).await
    }
    pub async fn update_status_missing_component(&self, client: Client, manager: &str, errors: Vec<String>) -> Result<Install, kube::Error> {
        self.update_status_typed(client, manager, errors, STATUS_MISSING_COMP).await
    }
    pub async fn update_status_missing_provider(&self, client: Client, manager: &str, errors: Vec<String>) -> Result<Install, kube::Error> {
        self.update_status_typed(client, manager, errors, STATUS_MISSING_PROV).await
    }
    pub async fn update_status_missing_dependencies(&self, client: Client, manager: &str, errors: Vec<String>) -> Result<Install, kube::Error> {
        self.update_status_typed(client, manager, errors, STATUS_MISSING_DEPS).await
    }
    pub async fn update_status_waiting_dependencies(&self, client: Client, manager: &str, errors: Vec<String>) -> Result<Install, kube::Error> {
        self.update_status_typed(client, manager, errors, STATUS_WAITING_DEPS).await
    }
    pub async fn update_status_installing(&self, client: Client, manager: &str) -> Result<Install, kube::Error> {
        self.update_status_typed(client, manager, Vec::new(), STATUS_INSTALLING).await
    }
    pub async fn update_status_planning(&self, client: Client, manager: &str) -> Result<Install, kube::Error> {
        self.update_status_typed(client, manager, Vec::new(), STATUS_PLANNING).await
    }
    pub async fn update_status_plan(&self, client: Client, manager: &str, plan: serde_json::Map<String, serde_json::Value>) -> Result<Install, kube::Error> {
        let name = self.name();
        let insts: Api<Install> = Api::namespaced(client, self.metadata.namespace.clone().unwrap().as_str());
        let last_updated = Utc::now();
        let new_status = Patch::Apply(json!({
            "apiVersion": "vynil.solidite.fr/v1",
            "kind": "Install",
            "status": InstallStatus {
                status: STATUS_PLANNED.to_string(),
                errors: Some(Vec::new()),
                tfstate: Some(self.current_tfstate()),
                planned: true,
                plan: Some(plan),
                last_updated,
                digest: self.options_digest()
            }
        }));
        let ps = PatchParams::apply(manager).force();
        insts.patch_status(&name, &ps, &new_status).await
    }
    pub async fn update_status_apply(&self, client: Client, manager: &str, tfstate: serde_json::Map<String, serde_json::Value>) -> Result<Install, kube::Error> {
        let name = self.name();
        let insts: Api<Install> = Api::namespaced(client, self.metadata.namespace.clone().unwrap().as_str());
        let last_updated = Utc::now();
        let new_status = Patch::Apply(json!({
            "apiVersion": "vynil.solidite.fr/v1",
            "kind": "Install",
            "status": InstallStatus {
                status: STATUS_INSTALLED.to_string(),
                errors: Some(Vec::new()),
                plan: Some(self.current_plan()),
                planned: false,
                tfstate: Some(tfstate),
                last_updated,
                digest: self.options_digest()
            }
        }));
        let ps = PatchParams::apply(manager).force();
        insts.patch_status(&name, &ps, &new_status).await
    }
}
