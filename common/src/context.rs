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
    pub static ref CLIENT_NAME: Mutex<String> = Mutex::new("vynil.solidite.fr".to_string());
    pub static ref KUBERNETES: Mutex<Option<Context>> = Mutex::new(None);
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
        VynilContext::TenantInstance(i) => Some(serde_json::json!({
            "app.kubernetes.io/managed-by": "vynil",
            "app.kubernetes.io/name": i.spec.package,
            "app.kubernetes.io/instance": i.metadata.name.clone().unwrap_or_default(),
            "vynil.solidite.fr/owner-namespace": i.metadata.namespace.clone().unwrap_or_default(),
            "vynil.solidite.fr/owner-category": i.spec.category,
            "vynil.solidite.fr/owner-type": "tenant"
        })),
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
pub fn set_agent() {
    *CLIENT_NAME.lock().unwrap() = "agent.vynil.solidite.fr".to_string()
}
pub fn set_controller() {
    *CLIENT_NAME.lock().unwrap() = "controller.vynil.solidite.fr".to_string()
}
pub fn get_client_name() -> String {
    CLIENT_NAME.lock().unwrap().to_string()
}
pub fn get_short_name() -> String {
    let long = get_client_name();
    let lst = long.split(".").collect::<Vec<&str>>();
    if lst.len() > 1 {
        format!("{}-{}", lst[1], lst[0])
    } else {
        "vynil".to_string()
    }
}

pub fn init_k8s() {
    *KUBERNETES.lock().unwrap() = Some(Context {
        client: tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async move { Client::try_default().await.expect("create client") })
        }),
        reporter: Reporter {
            controller: get_short_name(),
            instance: Some(std::env::var("POD_NAME").unwrap_or_else(|_| "unknown".to_string())),
        },
    });
}
pub fn get_client() -> Client {
    match (*KUBERNETES.lock().unwrap()).clone() {
        Some(ctx) => ctx.client,
        None => tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async move { Client::try_default().await.expect("create client") })
        }),
    }
}
pub fn get_reporter() -> Reporter {
    match (*KUBERNETES.lock().unwrap()).clone() {
        Some(ctx) => ctx.reporter,
        None => Reporter {
            controller: get_short_name(),
            instance: Some(std::env::var("POD_NAME").unwrap_or_else(|_| "unknown".to_string())),
        },
    }
}
