use kube::{api::{Api, Patch, PatchParams}, CustomResource, Client};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use serde_json::json;

pub const STATUS_ERRORS: &str = "errors";
pub const STATUS_INSTALLING: &str = "installing";
pub const STATUS_INSTALLED: &str = "installed";
pub const STATUS_PLANNING: &str = "planning";
pub const STATUS_PLANNED: &str = "planned";
pub const STATUS_TEMPLATING: &str = "templating";
pub const STATUS_TEMPLATED: &str = "templated";
pub const STATUS_DESTROYING: &str = "destroying";
pub const STATUS_DESTROYED: &str = "destroyed";
pub const STATUS_AGENT_STARTED: &str = "agent started";
pub const STATUS_MISSING_DIST: &str = "missing distribution";
pub const STATUS_MISSING_COMP: &str = "missing component";
pub const STATUS_CHECK_FAIL: &str = "Validations failed";
pub const STATUS_CONDITIONS_FAIL: &str = "Conditions script had errors";
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
{"name":"comp",   "type":"string", "description":"Component", "jsonPath":".spec.component"},
{"name":"status", "type":"string", "description":"Status", "jsonPath":".status.status"},
{"name":"errors", "type":"string", "description":"Errors", "jsonPath":".status.errors[*]"},
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
    /// Should we automatically upgrade the package
    pub auto_upgrade: Option<bool>,
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
    /// component version applied
    pub commit_id: String,
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
    pub fn namespace(&self) -> String {
        if let Some(ref name) = self.metadata.namespace {
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
        if let Some(p) = self.spec.auto_upgrade {
            if let Some(status) = self.status.clone() {
                // Do not auto-install if auto_upgrade is disabled and there is already a valid installation status
                ! status.commit_id.is_empty() && !p
            } else {false}
        } else {false}
    }
    fn current_plan(&self) -> serde_json::Map<String, serde_json::Value> {
        if let Some(ref status) = self.status {
            if let Some(ref plan) = status.plan {
                plan.clone()
            } else { serde_json::Map::new() }
        } else { serde_json::Map::new() }
    }
    fn current_commit_id(&self) -> String {
        if let Some(ref status) = self.status {
            status.commit_id.clone()
        } else { String::new() }
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

    pub async fn update_status_typed(&self, client: Client, manager: &str, errors: Vec<String>, typed: &str) -> Result<Install, kube::Error> {
        let name = self.name();
        let insts: Api<Install> = Api::namespaced(client, self.metadata.namespace.clone().unwrap().as_str());
        let last_updated = self.last_updated();
        let pp = if self.status.is_some() {PatchParams::apply(manager)} else {PatchParams::apply(manager).force()};
        let patch = if self.status.is_some() {
            Patch::Merge(serde_json::json!({
                "status": {
                    "errors": Some(errors),
                    "status": typed.to_string(),
                }
            }))
        } else {
            Patch::Apply(json!({
                "apiVersion": "vynil.solidite.fr/v1",
                "kind": "Install",
                "status": InstallStatus {
                    status: typed.to_string(),
                    plan: Some(self.current_plan()),
                    tfstate: Some(self.current_tfstate()),
                    errors: Some(errors),
                    commit_id: self.current_commit_id(),
                    last_updated,
                    digest: String::new()
                }
            }))
        };
        insts.patch_status(&name, &pp, &patch).await
    }
    // Update status for the operator
    pub async fn update_status_agent_started(&self, client: Client, manager: &str) -> Result<Install, kube::Error> {
            self.update_status_typed(client, manager, Vec::new(), STATUS_AGENT_STARTED).await
    }
    pub async fn update_status_missing_distrib(&self, client: Client, manager: &str, errors: Vec<String>) -> Result<Install, kube::Error> {
        self.update_status_typed(client, manager, errors, STATUS_MISSING_DIST).await
    }
    pub async fn update_status_missing_component(&self, client: Client, manager: &str, errors: Vec<String>) -> Result<Install, kube::Error> {
        self.update_status_typed(client, manager, errors, STATUS_MISSING_COMP).await
    }
    pub async fn update_status_check_failed(&self, client: Client, manager: &str, errors: Vec<String>) -> Result<Install, kube::Error> {
        self.update_status_typed(client, manager, errors, STATUS_CHECK_FAIL).await
    }
    pub async fn update_status_conditions_failed(&self, client: Client, manager: &str, errors: Vec<String>) -> Result<Install, kube::Error> {
        self.update_status_typed(client, manager, errors, STATUS_CONDITIONS_FAIL).await
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

    // Update Status for the agent
    async fn update_status_starting(&self, client: Client, manager: &str, typed: &str) -> Result<Install, kube::Error> {
        self.update_status_typed(client, manager, Vec::new(), typed).await
    }
    pub async fn update_status_start_plan(&self, client: Client, manager: &str) -> Result<Install, kube::Error> {
        self.update_status_starting(client, manager, STATUS_PLANNING).await
    }
    pub async fn update_status_start_destroy(&self, client: Client, manager: &str) -> Result<Install, kube::Error> {
        self.update_status_starting(client, manager, STATUS_DESTROYING).await
    }
    pub async fn update_status_start_install(&self, client: Client, manager: &str) -> Result<Install, kube::Error> {
        self.update_status_starting(client, manager, STATUS_INSTALLING).await
    }
    pub async fn update_status_start_template(&self, client: Client, manager: &str) -> Result<Install, kube::Error> {
        self.update_status_starting(client, manager, STATUS_TEMPLATING).await
    }
    pub async fn update_status_end_template(&self, client: Client, manager: &str) -> Result<Install, kube::Error> {
        self.update_status_starting(client, manager, STATUS_TEMPLATED).await
    }
    pub async fn update_status_end_destroy(&self, client: Client, manager: &str) -> Result<Install, kube::Error> {
        let name = self.name();
        let insts: Api<Install> = Api::namespaced(client, self.metadata.namespace.clone().unwrap().as_str());
        let last_updated = self.last_updated();
        let pp = if self.status.is_some() {PatchParams::apply(manager)} else {PatchParams::apply(manager).force()};
        let patch = if self.status.is_some() {
            Patch::Merge(serde_json::json!({
                "status": {
                    "plan": None::<Option<serde_json::Map<String, serde_json::Value>>>,
                    "tfstate": None::<Option<serde_json::Map<String, serde_json::Value>>>,
                    "errors": None::<Vec<String>>,
                    "status": STATUS_DESTROYED.to_string(),
                }
            }))
        } else {
            Patch::Apply(json!({
                "apiVersion": "vynil.solidite.fr/v1",
                "kind": "Install",
                "status": InstallStatus {
                    status: STATUS_DESTROYED.to_string(),
                    plan: None,
                    tfstate: None,
                    errors: Some(Vec::new()),
                    commit_id: self.current_commit_id(),
                    last_updated,
                    digest: String::new()
                }
            }))
        };
        insts.patch_status(&name, &pp, &patch).await
    }
    pub async fn update_status_errors(&self, client: Client, manager: &str, errors: Vec<String>) -> Result<Install, kube::Error> {
        self.update_status_typed(client, manager, errors, STATUS_ERRORS).await
    }
    pub async fn update_status_plan(&self, client: Client, manager: &str, plan: serde_json::Map<String, serde_json::Value>) -> Result<Install, kube::Error> {
        let name = self.name();
        let insts: Api<Install> = Api::namespaced(client, self.metadata.namespace.clone().unwrap().as_str());
        let no_errors: Vec<String> = Vec::new();
        let pp = PatchParams::apply(manager);
        let patch = Patch::Merge(serde_json::json!({
            "status": {
                "plan": Some(plan),
                "digest": self.options_digest(),
                "errors": Some(no_errors),
                "status": STATUS_PLANNED.to_string(),
            }
        }));
        insts.patch_status(&name, &pp, &patch).await
    }

    pub async fn update_status_errors_tfstate(&self, client: Client, manager: &str, errors: Vec<String>, tfstate: serde_json::Map<String, serde_json::Value>) -> Result<Install, kube::Error> {
        let name = self.name();
        let insts: Api<Install> = Api::namespaced(client, self.metadata.namespace.clone().unwrap().as_str());
        let last_updated = self.last_updated();
        let pp = if self.status.is_some() {PatchParams::apply(manager)} else {PatchParams::apply(manager).force()};
        let patch = if self.status.is_some() {
            Patch::Merge(serde_json::json!({
                "status": {
                    "errors": Some(errors),
                    "tfstate": Some(tfstate),
                    "status": STATUS_ERRORS.to_string(),
                }
            }))
        } else {
            // This shouldnt happen but still... we're saving the tfstate
            Patch::Apply(json!({
                "apiVersion": "vynil.solidite.fr/v1",
                "kind": "Install",
                "status": InstallStatus {
                    status: STATUS_ERRORS.to_string(),
                    plan: Some(self.current_plan()),
                    tfstate: Some(tfstate),
                    errors: Some(errors),
                    commit_id: self.current_commit_id(),
                    last_updated,
                    digest: String::new()
                }
            }))
        };
        insts.patch_status(&name, &pp, &patch).await
    }

    // TODO: should actually set the digest value (or remove that field from status)
    pub async fn update_status_apply(&self, client: Client, manager: &str, tfstate: serde_json::Map<String, serde_json::Value>, commit_id: String) -> Result<Install, kube::Error> {
        let name = self.name();
        let insts: Api<Install> = Api::namespaced(client, self.metadata.namespace.clone().unwrap().as_str());
        let pp = PatchParams::apply(manager).force();
        let no_errors: Vec<String> = Vec::new();
        let patch = Patch::Apply(json!({
            "apiVersion": "vynil.solidite.fr/v1",
            "kind": "Install",
            "status": InstallStatus {
                status: STATUS_INSTALLED.to_string(),
                plan: Some(serde_json::Map::new()),
                tfstate: Some(tfstate),
                errors: Some(no_errors),
                commit_id,
                last_updated: Utc::now(),
                digest: self.options_digest()
            }
        }));
        insts.patch_status(&name, &pp, &patch).await
    }
}
