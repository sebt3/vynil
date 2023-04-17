use crate::{distrib, install, Metrics};
use chrono::{DateTime, Utc};
use futures::{future::BoxFuture, FutureExt, StreamExt};
use kube::{
    api::{Api, ListParams},
    client::Client,
    runtime::{
        controller::Controller,
        events::Reporter,
        watcher::Config,
    },
};
use serde::Serialize;
use std::sync::Arc;
use tokio::{sync::RwLock};

// Context for our reconciler
#[derive(Clone)]
pub struct Context {
    /// Kubernetes client
    pub client: Client,
    /// Diagnostics read by the web server
    pub diagnostics: Arc<RwLock<Diagnostics>>,
    /// Prometheus metrics
    pub metrics: Metrics,
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
            reporter: "dist-controller".into(),
        }
    }
}

/// Data owned by the Manager
#[derive(Clone, Default)]
pub struct Manager {
    /// Diagnostics populated by the reconciler
    diagnostics: Arc<RwLock<Diagnostics>>,
}

/// Manager that owns a Controller for Distrib and Install
impl Manager {
    /// Lifecycle initialization interface for app
    ///
    /// This returns a `Manager` that drives a `Controller` + a future to be awaited
    /// It is up to `main` to wait for the controller stream.
    pub async fn new() -> (Self, BoxFuture<'static, ()>, BoxFuture<'static, ()>) {
        let client = Client::try_default().await.expect("create client");
        let manager = Manager::default();
        let context = Arc::new(Context {
            client: client.clone(),
            metrics: Metrics::default(),
            diagnostics: manager.diagnostics.clone(),
        });

        let dists = Api::<distrib::Distrib>::all(client.clone());
        let insts = Api::<install::Install>::all(client);
        // Ensure CRD is installed before loop-watching
        let _r = dists
            .list(&ListParams::default().limit(1))
            .await
            .expect("is the crd installed? please run: cargo run --bin crdgen | kubectl apply -f -");
        let _r = insts
            .list(&ListParams::default().limit(1))
            .await
            .expect("is the crd installed? please run: cargo run --bin crdgen | kubectl apply -f -");

        // All good. Start controller and return its future.
        let controller_dist = Controller::new(dists, Config::default().any_semantic())
            .run(distrib::reconcile, distrib::error_policy, context.clone())
            .filter_map(|x| async move { std::result::Result::ok(x) })
            .for_each(|_| futures::future::ready(()))
            .boxed();

        // All good. Start controller and return its future.
        let controller_inst = Controller::new(insts, Config::default().any_semantic())
            .run(install::reconcile, install::error_policy, context)
            .filter_map(|x| async move { std::result::Result::ok(x) })
            .for_each(|_| futures::future::ready(()))
            .boxed();

        (manager, controller_dist, controller_inst)
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
