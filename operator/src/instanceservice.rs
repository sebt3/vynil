use crate::{
    Error, Reconciler, Result, ServiceInstance,
    instance_common::{
        InstanceKind, RecoContext, build_base_recommendations, do_cleanup, do_reconcile, run_with_finalizer,
    },
    manager::Context,
    metrics::ReconcileMeasurerInstance,
};
use async_trait::async_trait;
use common::{
    rhaihandler::Script,
    vynilpackage::{VynilPackageRecommandation, VynilPackageRequirement, VynilPackageType},
};
use kube::{Client, runtime::controller::Action};
use opentelemetry::trace::TraceId;
use std::sync::Arc;
use tokio::time::Duration;
use tracing::instrument;

// ── InstanceKind implementation ───────────────────────────────────────────────

#[async_trait]
impl InstanceKind for ServiceInstance {
    fn type_name() -> &'static str {
        "service"
    }

    fn finalizer_name() -> &'static str {
        "serviceinstances.vynil.solidite.fr"
    }

    fn package_type() -> VynilPackageType {
        VynilPackageType::Service
    }

    fn spec_jukebox(&self) -> &str {
        &self.spec.jukebox
    }

    fn spec_category(&self) -> &str {
        &self.spec.category
    }

    fn spec_package(&self) -> &str {
        &self.spec.package
    }

    fn current_tag(&self) -> String {
        self.status
            .as_ref()
            .and_then(|s| s.tag.clone())
            .unwrap_or_default()
    }

    fn have_child(&self) -> bool {
        self.have_child()
    }

    fn get_options_digest(&mut self) -> String {
        self.get_options_digest()
    }

    async fn set_missing_box(mut self, jukebox: String) -> Result<Self> {
        self.set_missing_box(jukebox).await
    }

    async fn set_missing_package(mut self, category: String, package: String) -> Result<Self> {
        self.set_missing_package(category, package).await
    }

    async fn set_missing_requirement(mut self, reason: String) -> Result<Self> {
        self.set_missing_requirement(reason).await
    }

    async fn check_requirements(
        &self,
        reqs: Vec<VynilPackageRequirement>,
        client: Client,
    ) -> Result<Option<Action>> {
        for req in reqs {
            let (res, mes, requeue) = req.check_service(self, client.clone()).await?;
            if !res {
                self.clone().set_missing_requirement(mes).await?;
                return Ok(Some(Action::requeue(Duration::from_secs(requeue))));
            }
        }
        Ok(None)
    }

    async fn build_recommendations(
        &self,
        recos: Option<Vec<VynilPackageRecommandation>>,
        client: Client,
    ) -> Result<RecoContext> {
        let (crds, system_services) = build_base_recommendations(recos, client).await?;
        Ok(RecoContext {
            crds,
            system_services,
            tenant_services: Vec::new(),
        })
    }

    fn set_rhai_instance(&self, rhai: &mut Script) {
        rhai.ctx.set_value("instance", self.clone());
    }

    fn count_and_measure_metrics(&self, ctx: &Context, trace_id: &TraceId) -> ReconcileMeasurerInstance {
        ctx.metrics.service_instance.count_and_measure(self, trace_id)
    }

    fn record_reconcile_failure(&self, ctx: &Context, error: &Error) {
        ctx.metrics.service_instance.reconcile_failure(self, error);
    }
}

// ── Reconciler implementation ─────────────────────────────────────────────────

#[async_trait]
impl Reconciler for ServiceInstance {
    async fn reconcile(&self, ctx: Arc<Context>) -> Result<Action> {
        do_reconcile(self, ctx).await
    }

    async fn cleanup(&self, ctx: Arc<Context>) -> Result<Action> {
        do_cleanup(self, ctx).await
    }
}

// ── Controller entry points ───────────────────────────────────────────────────

#[instrument(skip(ctx, inst), fields(trace_id))]
pub async fn reconcile(inst: Arc<ServiceInstance>, ctx: Arc<Context>) -> Result<Action> {
    run_with_finalizer(inst, ctx).await
}

#[must_use]
pub fn error_policy(inst: Arc<ServiceInstance>, error: &Error, ctx: Arc<Context>) -> Action {
    tracing::warn!(
        "reconcile failed for ServiceInstance '{:?}.{:?}': {:?}",
        inst.metadata.namespace,
        inst.metadata.name,
        error
    );
    inst.record_reconcile_failure(&ctx, error);
    Action::requeue(Duration::from_secs(5 * 60))
}
