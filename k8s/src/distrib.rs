use kube::{api::{Api, Patch, PatchParams}, CustomResource, Client};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use serde_json::json;
pub use package::yaml::{Component, ComponentDependency};
use std::collections::HashMap;

/// Distrib:
///
/// Describe a source of components distribution git repository
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(kind = "Distrib", group = "vynil.solidite.fr", version = "v1")]
#[kube(status = "DistribStatus", shortname = "dist", printcolumn = r#"
    {"name":"url", "type":"string", "description":"Git url", "jsonPath":".spec.url"},
    {"name":"last_updated", "type":"string", "description":"Last update date", "format": "date-time", "jsonPath":".status.last_updated"}"#)]
pub struct DistribSpec {
    /// Git clone URL
    pub url: String,
    /// Git clone URL
    pub insecure: Option<bool>,
    /// Git branch
    pub branch: Option<String>,
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
    pub components: HashMap<String, HashMap<String, Component>>,
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

    pub fn components(&self) -> HashMap<String, HashMap<String, Component>> {
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
    pub fn get_component(&self, category: &str, component: &str) -> Option<Component> {
        if self.have_component(category, component) {
            Some(self.components()[category][component].clone())
        } else {
            None
        }
    }
    pub async fn update_status_components(&self, client: Client, manager: &str, components: HashMap<String, HashMap<String, Component>>) -> Result<Distrib, kube::Error> {
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
