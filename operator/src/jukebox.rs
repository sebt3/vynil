use crate::{manager::Context, telemetry, Error, Result, Reconciler, JukeBox};
use chrono::Utc;
use kube::{
    api::{Api, ResourceExt},
    runtime::{
        controller::Action,
        finalizer::{finalizer, Event as Finalizer},
    },
};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};
use tokio::time::Duration;
use tracing::{Span, field, instrument, warn};
use async_trait::async_trait;
use common::rhaihandler::{to_dynamic, Map, Script};

static JUKEBOX_FINALIZER: &str = "jukeboxes.vynil.solidite.fr";

#[instrument(skip(ctx, dist), fields(trace_id))]
pub async fn reconcile(dist: Arc<JukeBox>, ctx: Arc<Context>) -> Result<Action> {
    let trace_id = telemetry::get_trace_id();
    Span::current().record("trace_id", &field::display(&trace_id));
    let _mes = ctx.metrics.jukebox_count_and_measure();
    let dists: Api<JukeBox> = Api::all(ctx.client.clone());

    finalizer(&dists, JUKEBOX_FINALIZER, dist, |event| async {
        match event {
            Finalizer::Apply(dist) => dist.reconcile(ctx.clone()).await,
            Finalizer::Cleanup(dist) => dist.cleanup(ctx.clone()).await,
        }
    }).await.map_err(|e| Error::FinalizerError(Box::new(e)))
}

#[async_trait]
impl Reconciler for JukeBox {
    // Reconcile (for non-finalizer related changes)
    async fn reconcile(&self, ctx: Arc<Context>) -> Result<Action> {
        ctx.diagnostics.write().await.last_event = Utc::now();
        let mut rhai = Script::new(vec![]);
        rhai.set_dynamic("context", &serde_json::to_value(ctx.base_context.clone()).unwrap());
        rhai.ctx.set_value("box", self.clone());
        rhai.ctx.set_value("hbs", ctx.renderer.clone());
        match rhai.eval(&ctx.scripts["boxes/install"]) {
            Ok(_) => {
                tracing::info!("Updating packages cache");
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
                        *ctx.packages.write().await = map;
                    },
                    Err(e) => tracing::warn!("While listing jukebox: {:?}", e)
                };
                Ok(Action::requeue(Duration::from_secs(15 * 60)))
            },
            Err(e) => {
                warn!("While reconcile JukeBox {}: {e}", self.name_any());
                match e {
                    // TODO: better error handling
                    e => Err(e)
                }
            }
        }
    }

    // Reconcile with finalize cleanup (the object was deleted)
    async fn cleanup(&self, ctx: Arc<Context>) -> Result<Action> {
        ctx.diagnostics.write().await.last_event = Utc::now();
        let mut rhai = Script::new(vec![]);
        rhai.set_dynamic("context", &serde_json::to_value(ctx.base_context.clone()).unwrap());
        rhai.ctx.set_value("box", self.clone());
        rhai.ctx.set_value("hbs", ctx.renderer.clone());
        match rhai.eval(&ctx.scripts["boxes/delete"]) {
            Ok(_) => Ok(Action::await_change()),
            Err(e) => {
                warn!("While cleanup JukeBox {}: {e}", self.name_any());
                match e {
                    // TODO: better error handling
                    e => Err(e)
                }
            }
        }
    }
}

#[must_use] pub fn error_policy(dist: Arc<JukeBox>, error: &Error, ctx: Arc<Context>) -> Action {
    warn!("reconcile failed for {:?}: {:?}", dist.metadata.name, error);
    ctx.metrics.jukebox_reconcile_failure(&dist, error);
    Action::requeue(Duration::from_secs(5 * 60))
}
