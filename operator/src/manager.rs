use crate::{jukebox, JukeBox, instancesystem, SystemInstance, instancetenant, TenantInstance, Metrics};
use common::{handlebarshandler::HandleBars, rhaihandler::{Map, to_dynamic}};
use std::{sync::Arc, path::PathBuf, collections::BTreeMap};
use chrono::{DateTime, Utc};
use futures::{future::BoxFuture, FutureExt, StreamExt};
use kube::{
    api::{Api, ListParams},
    client::Client,
    runtime::{
        controller::Controller,
        events::Reporter,
        watcher::Config,
    }, ResourceExt,
};
use serde_json::{json, Value};
use serde::Serialize;
use tokio::sync::RwLock;
static DEFAULT_AGENT_IMAGE: &str = "docker.io/sebt3/vynil-agent:0.3.0";


// Context for our reconciler
#[derive(Clone)]
pub struct Context {
    /// Kubernetes client
    pub client: Client,
    /// Diagnostics read by the web server
    pub diagnostics: Arc<RwLock<Diagnostics>>,
    /// Prometheus metrics
    pub metrics: Metrics,
    /// handlebars renderer
    pub renderer: HandleBars<'static>,
    /// Rhai scripts
    pub scripts: BTreeMap<String, String>,
    /// Base context
    pub base_context: Value,
    /// Packages cache
    pub packages: Arc<RwLock<Map>>,
}

/// Diagnostics to be exposed by the web server
#[derive(Clone, Serialize)]
pub struct Diagnostics {
    #[serde(deserialize_with = "from_ts")]
    pub last_event: DateTime<Utc>,
    #[serde(skip)]
    pub reporter: Reporter,
}
impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            last_event: Utc::now(),
            reporter: "vynil-controller".into(),
        }
    }
}

/// Data owned by the Manager
#[derive(Clone, Default)]
pub struct Manager {
    /// Diagnostics populated by the reconciler
    diagnostics: Arc<RwLock<Diagnostics>>,
}

/// Manager that owns a Controller for JukeBox, SystemInstance, and TenantInstance
impl Manager {
    pub async fn new() -> (Self, BoxFuture<'static, ()>, BoxFuture<'static, ()>, BoxFuture<'static, ()>) {
        let client = Client::try_default().await.expect("create client");
        let manager = Manager::default();
        let controller_dir = std::env::var("CONTROLLER_BASE_DIR").unwrap_or("./operator".to_string());
        let mut hbs = HandleBars::new();
        match hbs.register_partial_dir(PathBuf::from(format!("{}/templates",controller_dir))) {
            Ok(_) => (),
            Err(e) => tracing::warn!("Registering template generated: {e}")
        }
        let mut scripts: BTreeMap<String, String> = BTreeMap::<String, String>::new();
        for file in vec!["boxes/install", "boxes/delete", "system/delete", "system/install", "tenant/delete", "tenant/install"] {
            scripts.insert(file.to_string(), match std::fs::read_to_string(format!("{}/scripts/{file}.rhai",controller_dir)) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("Loading {file}.rhai failed with: {e}"); String::new()
            }});
        }
        let packages: Arc<RwLock<Map>> = Arc::default();
        match JukeBox::list().await {
            Ok(lst) => {
                let mut map: Map = BTreeMap::new();
                for i in lst.items {
                    if let Some(status) = i.status.clone() {
                        let pcks = if let Some(pull_secret) = i.spec.pull_secret.clone() {
                            let mut res: Vec<Value> = Vec::new();
                            for pck in status.packages {
                                let mut tmp = serde_json::to_value(pck).unwrap();
                                tmp["pull_secret"] = serde_json::to_value(pull_secret.clone()).unwrap();
                                res.push(tmp);
                            }
                            to_dynamic(serde_json::to_value(res).unwrap()).unwrap()
                        } else {
                            to_dynamic(serde_json::to_value(status.packages).unwrap()).unwrap()
                        };
                        map.insert(i.name_any().into(), pcks);
                    }
                }
                *packages.write().await = map;
            },
            Err(e) => tracing::warn!("While listing jukebox: {:?}", e)
        };

        let context = Arc::new(Context {
            client: client.clone(),
            metrics: Metrics::default(),
            diagnostics: manager.diagnostics.clone(),
            renderer: hbs,
            scripts,
            base_context: json!({
                "vynil_namespace": std::env::var("VYNIL_NAMESPACE").unwrap_or_else(|_| "vynil-system".to_string()),
                "agent_image": std::env::var("AGENT_IMAGE").unwrap_or_else(|_| DEFAULT_AGENT_IMAGE.to_string()),
                "service_account": std::env::var("AGENT_ACCOUNT").unwrap_or_else(|_| "vynil-agent".to_string()),
                "log_level": std::env::var("AGENT_LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
            }),
            packages,
        });

        let jbs = Api::<JukeBox>::all(client.clone());
        let tnts = Api::<TenantInstance>::all(client.clone());
        let stms = Api::<SystemInstance>::all(client);
        // Ensure CRD is installed before loop-watching
        let _r = jbs
            .list(&ListParams::default().limit(1))
            .await
            .expect("is the crd installed?");
        let _r = tnts
            .list(&ListParams::default().limit(1))
            .await
            .expect("is the crd installed?");
        let _r = stms
            .list(&ListParams::default().limit(1))
            .await
            .expect("is the crd installed?");

        // All good. Start controller and return its future.
        let controller_jbs = Controller::new(jbs, Config::default().any_semantic())
            .run(jukebox::reconcile, jukebox::error_policy, context.clone())
            .filter_map(|x| async move { std::result::Result::ok(x) })
            .for_each(|_| futures::future::ready(()))
            .boxed();
        let controller_tnts = Controller::new(tnts, Config::default().any_semantic())
            .run(instancetenant::reconcile, instancetenant::error_policy, context.clone())
            .filter_map(|x| async move { std::result::Result::ok(x) })
            .for_each(|_| futures::future::ready(()))
            .boxed();
        let controller_stms = Controller::new(stms, Config::default().any_semantic())
            .run(instancesystem::reconcile, instancesystem::error_policy, context.clone())
            .filter_map(|x| async move { std::result::Result::ok(x) })
            .for_each(|_| futures::future::ready(()))
            .boxed();
        (manager, controller_jbs, controller_tnts, controller_stms)
    }

    /// Metrics getter
    #[must_use] pub fn metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
        prometheus::default_registry().gather()
    }

    /// State getter
    pub async fn diagnostics(&self) -> Diagnostics {
        self.diagnostics.read().await.clone()
    }
}
