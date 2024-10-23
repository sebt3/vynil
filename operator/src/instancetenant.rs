use crate::{manager::Context, telemetry, Error, Result, Reconciler, TenantInstance};
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
static TENANT_FINALIZER: &str = "tenantinstances.vynil.solidite.fr";


#[instrument(skip(ctx, inst), fields(trace_id))]
pub async fn reconcile(inst: Arc<TenantInstance>, ctx: Arc<Context>) -> Result<Action> {
    let trace_id = telemetry::get_trace_id();
    Span::current().record("trace_id", &field::display(&trace_id));
    let _mes = ctx.metrics.system_count_and_measure();
    let ns = inst.namespace().unwrap_or_default(); // inst is namespace scoped
    let insts: Api<TenantInstance> = Api::namespaced(ctx.client.clone(), &ns);

    finalizer(&insts, TENANT_FINALIZER, inst, |event| async {
        match event {
            Finalizer::Apply(inst) => inst.reconcile(ctx.clone()).await,
            Finalizer::Cleanup(inst) => inst.cleanup(ctx.clone()).await,
        }
    }).await.map_err(|e| Error::FinalizerError(Box::new(e)))
}

#[async_trait]
impl Reconciler for TenantInstance {
    // Reconcile (for non-finalizer related changes)
    async fn reconcile(&self, ctx: Arc<Context>) -> Result<Action> {
        ctx.diagnostics.write().await.last_event = Utc::now();
        let mut rhai = Script::new(vec![]);
        rhai.set_dynamic("context", &serde_json::to_value(ctx.base_context.clone()).unwrap());
        rhai.ctx.set_value("instance", self.clone());
        rhai.ctx.set_value("hbs", ctx.renderer.clone());
        rhai.ctx.set_value("packages", Map::from(ctx.packages.read().await.clone()));
        match rhai.eval(&ctx.scripts["tenant/install"]) {
            Ok(_) => Ok(Action::requeue(Duration::from_secs(15 * 60))),
            Err(e) => {
                warn!("While reconcile TenantInstance {}/{}: {e}", self.namespace().unwrap_or_default(), self.name_any());
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
        match rhai.eval(&ctx.scripts["tenant/delete"]) {
            Ok(_) => Ok(Action::await_change()),
            Err(e) => {
                warn!("While cleanup TenantInstance {}/{}: {e}", self.namespace().unwrap_or_default(), self.name_any());
                match e {
                    // TODO: better error handling
                    _e => Ok(Action::requeue(Duration::from_secs(1 * 60)))
                }
            }
        }
    }
}

#[must_use] pub fn error_policy(inst: Arc<TenantInstance>, error: &Error, ctx: Arc<Context>) -> Action {
    warn!("reconcile failed for '{:?}.{:?}': {:?}", inst.metadata.namespace, inst.metadata.name, error);
    ctx.metrics.tenant_reconcile_failure(&inst, error);
    Action::requeue(Duration::from_secs(5 * 60))
}
