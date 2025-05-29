use crate::{instancesystem, instancetenant, jukebox, JukeBox, Metrics, SystemInstance, TenantInstance};
use chrono::{DateTime, Utc};
use common::{handlebarshandler::HandleBars, vynilpackage::VynilPackage};
use futures::{future::BoxFuture, FutureExt, StreamExt};
use kube::{
    api::{Api, ListParams, ObjectList},
    client::Client,
    runtime::{controller::Controller, events::Reporter, watcher::Config},
    ResourceExt,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;
static DEFAULT_AGENT_IMAGE: &str = "docker.io/sebt3/vynil-agent:0.4.1";

pub struct JukeCacheItem {
    pub pull_secret: Option<String>,
    pub packages: Vec<VynilPackage>,
}

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
    /// Base context
    pub base_context: Value,
    /// Packages cache
    pub packages: Arc<RwLock<BTreeMap<String, JukeCacheItem>>>,
}
impl Context {
    pub async fn set_package_cache(&self, list: &ObjectList<JukeBox>) {
        let mut cache = BTreeMap::new();
        for juke in list.items.clone() {
            if let Some(status) = juke.status.clone() {
                cache.insert(juke.name_any(), JukeCacheItem {
                    pull_secret: juke.spec.pull_secret.clone(),
                    packages: status.packages,
                });
            }
        }
        *self.packages.write().await = cache;
    }
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
    /// Metrics
    metrics: Arc<Metrics>,
}

/// Manager that owns a Controller for JukeBox, SystemInstance, and TenantInstance
impl Manager {
    pub async fn new() -> (
        Self,
        BoxFuture<'static, ()>,
        BoxFuture<'static, ()>,
        BoxFuture<'static, ()>,
    ) {
        let client = Client::try_default().await.expect("create client");
        let manager = Manager::default();
        let controller_dir = std::env::var("CONTROLLER_BASE_DIR").unwrap_or("./operator".to_string());
        let mut hbs = HandleBars::new();
        match hbs.register_partial_dir(PathBuf::from(format!("{}/templates", controller_dir))) {
            Ok(_) => (),
            Err(e) => tracing::warn!("Registering template generated: {e}"),
        }
        let packages: Arc<RwLock<BTreeMap<String, JukeCacheItem>>> = Arc::default();
        match JukeBox::list().await {
            Ok(list) => {
                let mut cache = BTreeMap::new();
                for juke in list.items.clone() {
                    if let Some(status) = juke.status.clone() {
                        cache.insert(juke.name_any(), JukeCacheItem {
                            pull_secret: juke.spec.pull_secret.clone(),
                            packages: status.packages,
                        });
                    }
                }
                *packages.write().await = cache;
            }
            Err(e) => tracing::warn!("While listing jukebox: {:?}", e),
        };

        let context = Arc::new(Context {
            client: client.clone(),
            metrics: Metrics::default(),
            diagnostics: manager.diagnostics.clone(),
            renderer: hbs,
            base_context: json!({
                "vynil_namespace": std::env::var("VYNIL_NAMESPACE").unwrap_or_else(|_| "vynil-system".to_string()),
                "agent_image": std::env::var("AGENT_IMAGE").unwrap_or_else(|_| DEFAULT_AGENT_IMAGE.to_string()),
                "service_account": std::env::var("AGENT_ACCOUNT").unwrap_or_else(|_| "vynil-agent".to_string()),
                "log_level": std::env::var("AGENT_LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
                "label_key": std::env::var("TENANT_LABEL").unwrap_or_else(|_| "vynil.solidite.fr/tenant".to_string()),
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
            .run(
                instancetenant::reconcile,
                instancetenant::error_policy,
                context.clone(),
            )
            .filter_map(|x| async move { std::result::Result::ok(x) })
            .for_each(|_| futures::future::ready(()))
            .boxed();
        let controller_stms = Controller::new(stms, Config::default().any_semantic())
            .run(
                instancesystem::reconcile,
                instancesystem::error_policy,
                context.clone(),
            )
            .filter_map(|x| async move { std::result::Result::ok(x) })
            .for_each(|_| futures::future::ready(()))
            .boxed();
        (manager, controller_jbs, controller_tnts, controller_stms)
    }

    /// Metrics getter
    pub fn metrics(&self) -> String {
        let mut buffer = String::new();
        let reg_box = &self.metrics.reg_box;
        let reg_sys = &self.metrics.reg_sys;
        let reg_tnt = &self.metrics.reg_tnt;
        prometheus_client::encoding::text::encode(&mut buffer, reg_box).unwrap();
        prometheus_client::encoding::text::encode(&mut buffer, reg_sys).unwrap();
        prometheus_client::encoding::text::encode(&mut buffer, reg_tnt).unwrap();
        buffer
    }

    /// State getter
    pub async fn diagnostics(&self) -> Diagnostics {
        self.diagnostics.read().await.clone()
    }
}
