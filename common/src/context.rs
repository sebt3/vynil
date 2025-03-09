use crate::{instancesystem::SystemInstance, instancetenant::TenantInstance, jukebox::JukeBox};
use kube::{client::Client, runtime::events::Reporter};
use std::sync::Mutex;

#[derive(Clone)]
pub struct Context {
    pub client: Client,
    pub reporter: Reporter,
}

pub enum VynilContext {
    JukeBox(JukeBox),
    TenantInstance(TenantInstance),
    SystemInstance(SystemInstance),
    None,
}

lazy_static::lazy_static! {
    pub static ref CONTEXT: Mutex<VynilContext> = Mutex::new(VynilContext::None);
}
pub fn set_tenant(i: TenantInstance) {
    *CONTEXT.lock().unwrap() = VynilContext::TenantInstance(i);
}
pub fn set_system(i: SystemInstance) {
    *CONTEXT.lock().unwrap() = VynilContext::SystemInstance(i);
}
pub fn set_box(jb: JukeBox) {
    *CONTEXT.lock().unwrap() = VynilContext::JukeBox(jb);
}
pub fn get_owner_ns() -> Option<String> {
    match &*CONTEXT.lock().unwrap() {
        VynilContext::TenantInstance(i) => Some(i.metadata.namespace.clone().unwrap_or_default()),
        VynilContext::SystemInstance(i) => Some(i.metadata.namespace.clone().unwrap_or_default()),
        VynilContext::JukeBox(_j) => None,
        VynilContext::None => None,
    }
}
pub fn get_owner() -> Option<serde_json::Value> {
    match &*CONTEXT.lock().unwrap() {
        VynilContext::TenantInstance(i) => Some(serde_json::json!({
            "apiVersion": "vynil.solidite.fr/v1".to_string(),
            "kind": "TenantInstance".to_string(),
            "name": i.metadata.name.clone().unwrap_or_default(),
            "uid": i.metadata.uid.clone().unwrap_or_default(),
            "blockOwnerDeletion": true,
            "controller": true,
        })),
        VynilContext::SystemInstance(i) => Some(serde_json::json!({
            "apiVersion": "vynil.solidite.fr/v1".to_string(),
            "kind": "SystemInstance".to_string(),
            "name": i.metadata.name.clone().unwrap_or_default(),
            "uid": i.metadata.uid.clone().unwrap_or_default(),
            "blockOwnerDeletion": true,
            "controller": true,
        })),
        VynilContext::JukeBox(j) => Some(serde_json::json!({
            "apiVersion": "vynil.solidite.fr/v1".to_string(),
            "kind": "JukeBox".to_string(),
            "name": j.metadata.name.clone().unwrap_or_default(),
            "uid": j.metadata.uid.clone().unwrap_or_default(),
            "blockOwnerDeletion": true,
            "controller": true,
        })),
        VynilContext::None => None,
    }
}
pub fn get_labels() -> Option<serde_json::Value> {
    match &*CONTEXT.lock().unwrap() {
        VynilContext::TenantInstance(i) => {
            let tenant = i.clone().rhai_get_tenant_name().unwrap_or(String::new());
            Some(serde_json::json!({
                "app.kubernetes.io/managed-by": "vynil",
                "app.kubernetes.io/name": i.spec.package,
                "app.kubernetes.io/instance": i.metadata.name.clone().unwrap_or_default(),
                "vynil.solidite.fr/owner-namespace": i.metadata.namespace.clone().unwrap_or_default(),
                "vynil.solidite.fr/owner-category": i.spec.category,
                "vynil.solidite.fr/owner-type": "tenant",
                "vynil.solidite.fr/tenant": tenant
            }))
        }
        VynilContext::SystemInstance(i) => Some(serde_json::json!({
            "app.kubernetes.io/managed-by": "vynil",
            "app.kubernetes.io/name": i.spec.package,
            "app.kubernetes.io/instance": i.metadata.name.clone().unwrap_or_default(),
            "vynil.solidite.fr/owner-namespace": i.metadata.namespace.clone().unwrap_or_default(),
            "vynil.solidite.fr/owner-category": i.spec.category,
            "vynil.solidite.fr/owner-type": "system"
        })),
        VynilContext::JukeBox(j) => Some(serde_json::json!({
            "app.kubernetes.io/managed-by": "vynil",
            "app.kubernetes.io/instance": j.metadata.name.clone().unwrap_or_default(),
            "vynil.solidite.fr/owner-type": "jukebox"
        })),
        VynilContext::None => None,
    }
}

fn get_prog_name() -> Option<String> {
    std::env::current_exe()
        .ok()?
        .file_name()?
        .to_str()?
        .to_owned()
        .into()
}

pub fn get_client_name() -> String {
    match get_prog_name() {
        None => "vynil.solidite.fr".to_string(),
        Some(p) => {
            if p == "agent".to_string() {
                "agent.vynil.solidite.fr".to_string()
            } else if p == "operator".to_string() {
                "controller.vynil.solidite.fr".to_string()
            } else {
                "vynil.solidite.fr".to_string()
            }
        }
    }
}
pub fn get_short_name() -> String {
    let long = get_client_name();
    let lst = long.split(".").collect::<Vec<&str>>();
    if lst.len() > 3 {
        format!("{}-{}", lst[1], lst[0])
    } else {
        "vynil".to_string()
    }
}

pub fn get_client() -> Client {
    tokio::runtime::Handle::current()
        .block_on(async move { Client::try_default().await.expect("create client") })
}
pub async fn get_client_async() -> Client {
    Client::try_default().await.expect("create client")
}
pub fn get_reporter() -> Reporter {
    Reporter {
        controller: get_short_name(),
        instance: Some(std::env::var("POD_NAME").unwrap_or_else(|_| "unknown".to_string())),
    }
}
