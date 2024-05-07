use kube::{api::{Api, Patch, PatchParams}, CustomResource, Client};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use serde_json::json;
pub use crate::yaml::{ComponentDependency, Providers};
use std::collections::HashMap;

/// Secret Reference
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct SecretRef {
    /// Name of the secret
    pub name: String,
    /// Key of the secret containing the file
    pub key: String
}

/// Distribution source authentication
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct DistribAuthent {
    /// SSH private key
    pub ssh_key: Option<SecretRef>,
    /// a git-credentials store file (format: https://<username>:<password|token>@<url>/<repo>)
    pub git_credentials: Option<SecretRef>,
}

/// Distribution Component
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct DistribComponent {
    /// last known commit_id
    pub commit_id: String,
    /// Component description
    pub description: Option<String>,
    /// Component options
    pub options: HashMap<String, serde_json::Value>,
    /// Component dependencies
    pub dependencies: Option<Vec<ComponentDependency>>,
    /// Component providers
    pub providers: Option<Providers>,
    /// Check Script
    pub check: Option<String>,
}

impl DistribComponent {
    pub fn new(commit_id: String, description: Option<String>, options: HashMap<String, serde_json::Value>, dependencies: Option<Vec<ComponentDependency>>, providers: Option<Providers>, check: Option<String>) -> Self {
        DistribComponent {
            commit_id,
            description,
            options,
            dependencies,
            providers,
            check
        }
    }
}

/// Distrib:
///
/// Describe a source of components distribution git repository
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(kind = "Distrib", group = "vynil.solidite.fr", version = "v1")]
#[kube(status = "DistribStatus", shortname = "dist", printcolumn = r#"
    {"name":"url", "type":"string", "description":"Git url", "jsonPath":".spec.url"},
    {"name":"branch", "type":"string", "description":"Git branch", "jsonPath":".spec.branch"},
    {"name":"schedule", "type":"string", "description":"Update schedule", "jsonPath":".spec.schedule"},
    {"name":"last_updated", "type":"string", "description":"Last update date", "format": "date-time", "jsonPath":".status.last_updated"}"#)]
pub struct DistribSpec {
    /// Git clone URL
    pub url: String,
    /// Git clone URL
    pub insecure: Option<bool>,
    /// Git branch
    pub branch: Option<String>,
    /// Git authentication
    pub login: Option<DistribAuthent>,
    /// Actual cron-type expression that defines the interval of the updates.
    pub schedule: String,
}
/// The status object of `Distrib`
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct DistribStatus {
    /// Set with the messages if any error occured
    pub errors: Option<Vec<String>>,
    /// Last update date
    pub last_updated: DateTime<Utc>,
    /// List of known category->components
    pub components: HashMap<String, HashMap<String, DistribComponent>>,
}

impl Distrib {
    pub fn name(&self) -> String {
        if let Some(ref name) = self.metadata.name {
            name.clone()
        } else {
            String::new()
        }
    }
    pub fn last_updated(&self) -> DateTime<Utc> {
        self.status.as_ref().map_or_else(Utc::now, |s| s.last_updated)
    }

    pub fn components(&self) -> HashMap<String, HashMap<String, DistribComponent>> {
        self.status.clone().map(|s| s.components).unwrap_or_default()
    }

    pub fn branch(&self) -> String {
        if let Some(ref branch) = self.spec.branch {branch.clone()} else {String::new()}
    }

    pub fn insecure(&self) -> bool {
        if let Some(ref i) = self.spec.insecure {*i} else {false}
    }
    pub fn have_component(&self, category: &str, component: &str) -> bool {
        if self.components().contains_key(category) {
            self.components()[category].contains_key(component)
        }
        else {
            false
        }
    }
    pub fn get_component(&self, category: &str, component: &str) -> Option<DistribComponent> {
        if self.have_component(category, component) {
            Some(self.components()[category][component].clone())
        } else {
            None
        }
    }
    pub async fn update_status_components(&self, client: Client, manager: &str, components: HashMap<String, HashMap<String, DistribComponent>>) -> Result<Distrib, kube::Error> {
        let name = self.metadata.name.clone().unwrap();
        let dists: Api<Distrib> = Api::all(client);
        let new_status = Patch::Apply(json!({
            "apiVersion": "vynil.solidite.fr/v1",
            "kind": "Distrib",
            "status": DistribStatus {
                errors: Some(Vec::new()),
                last_updated: Utc::now(),
                components
            }
        }));
        let ps = PatchParams::apply(manager).force();
        dists.patch_status(&name, &ps, &new_status).await
    }
    pub async fn update_status_errors(&self, client: Client, manager: &str, errors: Vec<String>) -> Result<Distrib, kube::Error> {
        let name = self.metadata.name.clone().unwrap();
        let dists: Api<Distrib> = Api::all(client);
        let components = self.components();
        let last_updated = self.last_updated();
        let new_status = Patch::Apply(json!({
            "apiVersion": "vynil.solidite.fr/v1",
            "kind": "Distrib",
            "status": DistribStatus {
                errors: Some(errors),
                last_updated,
                components
            }
        }));
        let ps = PatchParams::apply(manager).force();
        dists.patch_status(&name, &ps, &new_status).await
    }
}
