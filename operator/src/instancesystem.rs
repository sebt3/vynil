use crate::{manager::Context, telemetry, Error, Result, Reconciler, SystemInstance};
use common::rhaihandler::{Map, Script};
use chrono::Utc;
use kube::{
    api::{Api, ResourceExt},
    runtime::{
        controller::Action,
        finalizer::{finalizer, Event as Finalizer},
    },
};
use std::sync::Arc;
use tokio::time::Duration;
use tracing::{Span, field, instrument, warn};
use async_trait::async_trait;
static SYSTEM_FINALIZER: &str = "systeminstances.vynil.solidite.fr";


#[instrument(skip(ctx, inst), fields(trace_id))]
pub async fn reconcile(inst: Arc<SystemInstance>, ctx: Arc<Context>) -> Result<Action> {
    let trace_id = telemetry::get_trace_id();
    Span::current().record("trace_id", &field::display(&trace_id));
    let _mes = ctx.metrics.system_count_and_measure();
    let ns = inst.namespace().unwrap_or_default(); // inst is namespace scoped
    let insts: Api<SystemInstance> = Api::namespaced(ctx.client.clone(), &ns);

    finalizer(&insts, SYSTEM_FINALIZER, inst, |event| async {
        match event {
            Finalizer::Apply(inst) => inst.reconcile(ctx.clone()).await,
            Finalizer::Cleanup(inst) => inst.cleanup(ctx.clone()).await,
        }
    }).await.map_err(|e| Error::FinalizerError(Box::new(e)))
}

#[async_trait]
impl Reconciler for SystemInstance {
    // Reconcile (for non-finalizer related changes)
    async fn reconcile(&self, ctx: Arc<Context>) -> Result<Action> {
        ctx.diagnostics.write().await.last_event = Utc::now();
        let mut rhai = Script::new(vec![]);
        rhai.set_dynamic("context", &serde_json::to_value(ctx.base_context.clone()).unwrap());
        rhai.ctx.set_value("instance", self.clone());
        rhai.ctx.set_value("hbs", ctx.renderer.clone());
        rhai.ctx.set_value("packages", Map::from(ctx.packages.read().await.clone()));
        match rhai.eval(&ctx.scripts["system/install"]) {
            Ok(_v) => {
                Ok(Action::requeue(Duration::from_secs(15 * 60)))
            },
            Err(e) => {
                warn!("While reconcile SystemInstance {}/{}: {e}", self.namespace().unwrap_or_default(), self.name_any());
                match e {
                    // TODO: better error handling
                    _e => Ok(Action::requeue(Duration::from_secs(1 * 60)))
                }
            }
        }
    }

    // Reconcile with finalize cleanup (the object was deleted)
    async fn cleanup(&self, ctx: Arc<Context>) -> Result<Action> {
        ctx.diagnostics.write().await.last_event = Utc::now();
        let mut rhai = Script::new(vec![]);
        rhai.set_dynamic("context", &serde_json::to_value(ctx.base_context.clone()).unwrap());
        rhai.ctx.set_value("instance", self.clone());
        rhai.ctx.set_value("hbs", ctx.renderer.clone());
        rhai.ctx.set_value("packages", Map::from(ctx.packages.read().await.clone()));
        match rhai.eval(&ctx.scripts["system/delete"]) {
            Ok(_v) => {
                Ok(Action::await_change())
            },
            Err(e) => {
                warn!("While cleanup SystemInstance {}/{}: {e}", self.namespace().unwrap_or_default(), self.name_any());
                match e {
                    // TODO: better error handling
                    _e => Ok(Action::requeue(Duration::from_secs(1 * 60)))
                }
            }
        }
    }
}

#[must_use] pub fn error_policy(inst: Arc<SystemInstance>, error: &Error, ctx: Arc<Context>) -> Action {
    warn!("reconcile failed for '{:?}.{:?}': {:?}", inst.metadata.namespace, inst.metadata.name, error);
    ctx.metrics.system_reconcile_failure(&inst, error);
    Action::requeue(Duration::from_secs(5 * 60))
}
