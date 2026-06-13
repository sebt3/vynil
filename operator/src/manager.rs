use crate::{
    JukeBox, Metrics, ServiceInstance, SystemInstance, TenantInstance, instanceservice, instancesystem,
    instancetenant, jukebox,
};
use chrono::{DateTime, Utc};
use common::{handlebarshandler::HandleBars, vynilpackage::VynilPackage};
use futures::{FutureExt, StreamExt, future::BoxFuture};
use kube::{
    ResourceExt,
    api::{Api, ListParams, ObjectList},
    client::Client,
    runtime::{controller::Controller, events::Reporter, watcher::Config},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;

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
    pub metrics: Arc<Metrics>,
    /// handlebars renderer
    pub renderer: HandleBars<'static>,
    /// Base context
    pub base_context: Value,
    /// Packages cache
    pub packages: Arc<RwLock<BTreeMap<String, JukeCacheItem>>>,
}
pub(crate) fn cache_entry_differs(cache: &BTreeMap<String, JukeCacheItem>, jukebox: &JukeBox) -> bool {
    let Some(status) = &jukebox.status else {
        return false;
    };
    match cache.get(&jukebox.name_any()) {
        Some(entry) => entry.packages != status.packages || entry.pull_secret != jukebox.spec.pull_secret,
        None => true,
    }
}

pub(crate) fn upsert_cache_entry(cache: &mut BTreeMap<String, JukeCacheItem>, jukebox: &JukeBox) {
    let Some(status) = &jukebox.status else { return };
    cache.insert(jukebox.name_any(), JukeCacheItem {
        pull_secret: jukebox.spec.pull_secret.clone(),
        packages: status.packages.clone(),
    });
}

impl Context {
    pub async fn upsert_jukebox_cache(&self, jukebox: &JukeBox) {
        let mut cache = self.packages.write().await;
        upsert_cache_entry(&mut cache, jukebox);
    }

    pub async fn remove_jukebox_cache(&self, name: &str) {
        self.packages.write().await.remove(name);
    }

    pub async fn cache_needs_update(&self, jukebox: &JukeBox) -> bool {
        let cache = self.packages.read().await;
        cache_entry_differs(&cache, jukebox)
    }

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
        let jukes = list
            .items
            .clone()
            .iter()
            .map(|j| j.name_any())
            .reduce(|j, r| format!("{j},{r}"))
            .unwrap_or(String::new());
        if !cache.is_empty() {
            let len = cache.len();
            let mut count = 0;
            for items in cache.values() {
                count += items.packages.len();
            }
            tracing::info!("Updating packages cache with {count} packages from {len} jukebox: {jukes}");
            *self.packages.write().await = cache;
            tracing::debug!("Updating packages cache done");
        } else {
            tracing::warn!("No packages found from the jukebox list ({jukes}) to update the cache");
        }
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
                tracing::debug!("Initialize packages cache");
                *packages.write().await = cache;
                tracing::debug!("Initialize packages cache done");
            }
            Err(e) => tracing::warn!("While listing jukebox: {:?}", e),
        };

        let context = Arc::new(Context {
            client: client.clone(),
            metrics: manager.metrics.clone(),
            diagnostics: manager.diagnostics.clone(),
            renderer: hbs,
            base_context: json!({
                "vynil_namespace": std::env::var("VYNIL_NAMESPACE").unwrap_or_else(|_| "vynil-system".to_string()),
                "agent_image": std::env::var("AGENT_IMAGE").unwrap_or_else(|_| common::DEFAULT_AGENT_IMAGE.to_string()),
                "service_account": std::env::var("AGENT_ACCOUNT").unwrap_or_else(|_| "vynil-agent".to_string()),
                "log_level": std::env::var("AGENT_LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
                "label_key": std::env::var("TENANT_LABEL").unwrap_or_else(|_| "vynil.solidite.fr/tenant".to_string()),
            }),
            packages,
        });

        let jbs = Api::<JukeBox>::all(client.clone());
        let tnts = Api::<TenantInstance>::all(client.clone());
        let svcs = Api::<ServiceInstance>::all(client.clone());
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
        let _r = svcs
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
        let controller_svcs = Controller::new(svcs, Config::default().any_semantic())
            .run(
                instanceservice::reconcile,
                instanceservice::error_policy,
                context.clone(),
            )
            .filter_map(|x| async move { std::result::Result::ok(x) })
            .for_each(|_| futures::future::ready(()))
            .boxed();
        (
            manager,
            controller_jbs,
            controller_tnts,
            controller_stms,
            controller_svcs,
        )
    }

    /// Metrics getter
    pub fn metrics(&self) -> String {
        let mut buffer = String::new();
        prometheus_client::encoding::text::encode_registry(&mut buffer, &self.metrics.reg_box).unwrap();
        prometheus_client::encoding::text::encode_registry(&mut buffer, &self.metrics.reg_sys).unwrap();
        prometheus_client::encoding::text::encode_registry(&mut buffer, &self.metrics.reg_svc).unwrap();
        prometheus_client::encoding::text::encode_registry(&mut buffer, &self.metrics.reg_tnt).unwrap();
        prometheus_client::encoding::text::encode_eof(&mut buffer).unwrap();
        buffer
    }

    /// State getter
    pub async fn diagnostics(&self) -> Diagnostics {
        self.diagnostics.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::{
        jukebox::{JukeBoxSpec, JukeBoxStatus},
        vynilpackage::{VynilPackage, VynilPackageMeta, VynilPackageType},
    };
    use kube::api::ObjectMeta;

    fn make_pkg(category: &str, name: &str) -> VynilPackage {
        VynilPackage {
            registry: String::new(),
            image: String::new(),
            tag: String::new(),
            metadata: VynilPackageMeta {
                name: name.to_string(),
                category: category.to_string(),
                description: String::new(),
                app_version: None,
                usage: VynilPackageType::default(),
                features: vec![],
                backup_affinity: None,
            },
            requirements: vec![],
            recommandations: None,
            options: None,
            value_script: None,
        }
    }

    fn make_jukebox(name: &str, packages: Vec<VynilPackage>, pull_secret: Option<String>) -> JukeBox {
        JukeBox {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                ..Default::default()
            },
            spec: JukeBoxSpec {
                schedule: "0 * * * *".to_string(),
                pull_secret,
                source: None,
                maturity: None,
            },
            status: Some(JukeBoxStatus {
                conditions: vec![],
                packages,
            }),
        }
    }

    #[test]
    fn cache_entry_differs_when_not_in_cache() {
        let cache = BTreeMap::new();
        let jb = make_jukebox("box-a", vec![make_pkg("db", "pg")], None);
        assert!(cache_entry_differs(&cache, &jb));
    }

    #[test]
    fn cache_entry_differs_when_packages_changed() {
        let mut cache = BTreeMap::new();
        cache.insert("box-a".to_string(), JukeCacheItem {
            pull_secret: None,
            packages: vec![make_pkg("db", "old")],
        });
        let jb = make_jukebox("box-a", vec![make_pkg("db", "pg")], None);
        assert!(cache_entry_differs(&cache, &jb));
    }

    #[test]
    fn cache_entry_idempotent_when_status_unchanged() {
        let pkg = make_pkg("db", "pg");
        let mut cache = BTreeMap::new();
        cache.insert("box-a".to_string(), JukeCacheItem {
            pull_secret: None,
            packages: vec![pkg.clone()],
        });
        let jb = make_jukebox("box-a", vec![pkg], None);
        assert!(!cache_entry_differs(&cache, &jb));
    }

    #[test]
    fn cache_entry_differs_when_pull_secret_changed() {
        let pkg = make_pkg("db", "pg");
        let mut cache = BTreeMap::new();
        cache.insert("box-a".to_string(), JukeCacheItem {
            pull_secret: None,
            packages: vec![pkg.clone()],
        });
        let jb = make_jukebox("box-a", vec![pkg], Some("new-secret".to_string()));
        assert!(cache_entry_differs(&cache, &jb));
    }

    #[test]
    fn cache_entry_does_not_differ_when_no_status() {
        let cache = BTreeMap::new();
        let mut jb = make_jukebox("box-a", vec![], None);
        jb.status = None;
        assert!(!cache_entry_differs(&cache, &jb));
    }

    #[test]
    fn upsert_cache_entry_inserts_new() {
        let mut cache = BTreeMap::new();
        let jb = make_jukebox("box-a", vec![make_pkg("db", "pg")], None);
        upsert_cache_entry(&mut cache, &jb);
        assert!(cache.contains_key("box-a"));
        assert_eq!(cache["box-a"].packages.len(), 1);
    }

    #[test]
    fn upsert_cache_entry_updates_existing() {
        let mut cache = BTreeMap::new();
        cache.insert("box-a".to_string(), JukeCacheItem {
            pull_secret: None,
            packages: vec![make_pkg("db", "old")],
        });
        let jb = make_jukebox("box-a", vec![make_pkg("db", "new")], None);
        upsert_cache_entry(&mut cache, &jb);
        assert_eq!(cache["box-a"].packages[0].metadata.name, "new");
    }

    #[test]
    fn upsert_preserves_other_jukebox_entries() {
        let mut cache = BTreeMap::new();
        cache.insert("box-b".to_string(), JukeCacheItem {
            pull_secret: None,
            packages: vec![make_pkg("monitoring", "prom")],
        });
        let jb = make_jukebox("box-a", vec![make_pkg("db", "pg")], None);
        upsert_cache_entry(&mut cache, &jb);
        assert!(cache.contains_key("box-a"));
        assert!(cache.contains_key("box-b"));
    }

    #[test]
    fn upsert_no_op_when_no_status() {
        let mut cache = BTreeMap::new();
        let mut jb = make_jukebox("box-a", vec![], None);
        jb.status = None;
        upsert_cache_entry(&mut cache, &jb);
        assert!(!cache.contains_key("box-a"));
    }
}
