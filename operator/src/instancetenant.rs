use crate::{
    Error, Reconciler, Result, TenantInstance,
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
impl InstanceKind for TenantInstance {
    fn type_name() -> &'static str {
        "tenant"
    }

    fn finalizer_name() -> &'static str {
        "tenantinstances.vynil.solidite.fr"
    }

    fn package_type() -> VynilPackageType {
        VynilPackageType::Tenant
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
            let (res, mes, requeue) = req.check_tenant(self, client.clone()).await?;
            if !res {
                self.clone().set_missing_requirement(mes).await?;
                return Ok(Some(Action::requeue(Duration::from_secs(requeue))));
            }
        }
        Ok(None)
    }

    /// TenantInstance additionally handles the `TenantService` recommendation variant.
    async fn build_recommendations(
        &self,
        recos: Option<Vec<VynilPackageRecommandation>>,
        client: Client,
    ) -> Result<RecoContext> {
        let mut rec_tenant_services: Vec<String> = Vec::new();
        // Collect tenant service names before iterating over recos
        let current_tenant_services = self.get_tenant_services_names().await?;

        // Separate TenantService entries from the rest so that
        // build_base_recommendations can handle CRDs and SystemServices.
        let (tenant_recos, base_recos): (Vec<_>, Vec<_>) = recos
            .unwrap_or_default()
            .into_iter()
            .partition(|r| matches!(r, VynilPackageRecommandation::TenantService(_)));

        for reco in tenant_recos {
            if let VynilPackageRecommandation::TenantService(svc) = reco
                && current_tenant_services.contains(&svc)
            {
                rec_tenant_services.push(svc);
            }
        }
        rec_tenant_services.sort();

        let (crds, system_services) = build_base_recommendations(Some(base_recos), client).await?;
        Ok(RecoContext {
            crds,
            system_services,
            tenant_services: rec_tenant_services,
        })
    }

    fn set_rhai_instance(&self, rhai: &mut Script) {
        rhai.ctx.set_value("instance", self.clone());
    }

    fn count_and_measure_metrics(&self, ctx: &Context, trace_id: &TraceId) -> ReconcileMeasurerInstance {
        ctx.metrics.tenant_instance.count_and_measure(self, trace_id)
    }

    fn record_reconcile_failure(&self, ctx: &Context, error: &Error) {
        ctx.metrics.tenant_instance.reconcile_failure(self, error);
    }
}

// ── Reconciler implementation ─────────────────────────────────────────────────

#[async_trait]
impl Reconciler for TenantInstance {
    async fn reconcile(&self, ctx: Arc<Context>) -> Result<Action> {
        do_reconcile(self, ctx).await
    }

    async fn cleanup(&self, ctx: Arc<Context>) -> Result<Action> {
        do_cleanup(self, ctx).await
    }
}

// ── Controller entry points ───────────────────────────────────────────────────

#[instrument(skip(ctx, inst), fields(trace_id))]
pub async fn reconcile(inst: Arc<TenantInstance>, ctx: Arc<Context>) -> Result<Action> {
    run_with_finalizer(inst, ctx).await
}

#[must_use]
pub fn error_policy(inst: Arc<TenantInstance>, error: &Error, ctx: Arc<Context>) -> Action {
    tracing::warn!(
        "reconcile failed for TenantInstance '{:?}.{:?}': {:?}",
        inst.metadata.namespace,
        inst.metadata.name,
        error
    );
    inst.record_reconcile_failure(&ctx, error);
    Action::requeue(Duration::from_secs(5 * 60))
}
