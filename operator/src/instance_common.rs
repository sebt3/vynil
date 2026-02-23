use crate::{
    Error, Reconciler, Result, get_client_name, manager::Context, metrics::ReconcileMeasurerInstance,
    telemetry,
};
use async_trait::async_trait;
use chrono::Utc;
use common::{
    rhaihandler::Script,
    vynilpackage::{VynilPackageRecommandation, VynilPackageRequirement, VynilPackageType},
};
use k8s_openapi::{
    NamespaceResourceScope, api::batch::v1::Job,
    apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition,
};
use kube::{
    Client, ResourceExt,
    api::{Api, DeleteParams, Patch, PatchParams, PostParams},
    runtime::{
        conditions,
        controller::Action,
        finalizer::{Event as Finalizer, finalizer},
        wait::await_condition,
    },
};
use opentelemetry::trace::TraceId;
use serde_json::Value;
use std::sync::Arc;
use tokio::time::Duration;
use tracing::{Span, field};

// ── Recommendation context ────────────────────────────────────────────────────

/// Holds the three recommendation lists computed during reconciliation
pub struct RecoContext {
    pub crds: Vec<String>,
    pub system_services: Vec<String>,
    pub tenant_services: Vec<String>,
}

// ── InstanceKind trait ────────────────────────────────────────────────────────

/// Captures all behaviors that differ between the three instance kinds
/// (ServiceInstance, SystemInstance, TenantInstance).
///
/// The generic reconcile/cleanup logic lives in [`do_reconcile`] and [`do_cleanup`];
/// those functions call the methods below for every type-specific decision.
#[async_trait]
pub trait InstanceKind:
    kube::Resource<DynamicType = (), Scope = NamespaceResourceScope>
    + Clone
    + std::fmt::Debug
    + serde::de::DeserializeOwned
    + serde::Serialize
    + Send
    + Sync
    + 'static
{
    // ── Type-level constants ──────────────────────────────────────────────
    fn type_name() -> &'static str
    where
        Self: Sized;
    fn finalizer_name() -> &'static str
    where
        Self: Sized;
    fn package_type() -> VynilPackageType
    where
        Self: Sized;

    // ── Spec accessors ────────────────────────────────────────────────────
    fn spec_jukebox(&self) -> &str;
    fn spec_category(&self) -> &str;
    fn spec_package(&self) -> &str;
    /// Returns the currently installed tag from the status, or an empty string.
    fn current_tag(&self) -> String;
    fn have_child(&self) -> bool;
    fn get_options_digest(&mut self) -> String;

    // ── Status update methods ─────────────────────────────────────────────
    async fn set_missing_box(self, jukebox: String) -> Result<Self>;
    async fn set_missing_package(self, category: String, package: String) -> Result<Self>;
    async fn set_missing_requirement(self, reason: String) -> Result<Self>;

    // ── Type-specific behaviors ───────────────────────────────────────────

    /// Checks every requirement; returns `None` when all pass, or
    /// `Some(Action)` (a requeue) on the first failure, after recording
    /// the missing requirement in the resource status.
    async fn check_requirements(
        &self,
        reqs: Vec<VynilPackageRequirement>,
        client: Client,
    ) -> Result<Option<Action>>;

    /// Builds the three recommendation lists (CRDs, system services, tenant services).
    /// TenantInstance overrides this to also fill `tenant_services`.
    async fn build_recommendations(
        &self,
        recos: Option<Vec<VynilPackageRecommandation>>,
        client: Client,
    ) -> Result<RecoContext>;

    /// Registers `self` as the `instance` variable in the rhai scope.
    /// Implemented concretely by each type so the rhai engine's registered
    /// methods for that type remain available.
    fn set_rhai_instance(&self, rhai: &mut Script);

    // ── Metrics ───────────────────────────────────────────────────────────
    fn count_and_measure_metrics(&self, ctx: &Context, trace_id: &TraceId) -> ReconcileMeasurerInstance;
    fn record_reconcile_failure(&self, ctx: &Context, error: &Error);
}

// ── Shared recommendation helpers ─────────────────────────────────────────────

/// Builds the CRD and system-service recommendation lists that are common
/// to all three instance types.
pub async fn build_base_recommendations(
    recos: Option<Vec<VynilPackageRecommandation>>,
    client: Client,
) -> Result<(Vec<String>, Vec<String>)> {
    let mut rec_crds: Vec<String> = Vec::new();
    let mut rec_system_services: Vec<String> = Vec::new();
    if let Some(recos) = recos {
        let current_system_services =
            common::instanceservice::ServiceInstance::get_all_services_names().await?;
        for reco in recos {
            match reco {
                VynilPackageRecommandation::CustomResourceDefinition(crd) => {
                    let api: Api<CustomResourceDefinition> = Api::all(client.clone());
                    if api
                        .get_metadata_opt(&crd)
                        .await
                        .map_err(Error::KubeError)?
                        .is_some()
                    {
                        rec_crds.push(crd);
                    }
                }
                VynilPackageRecommandation::SystemService(svc) => {
                    if current_system_services.contains(&svc) {
                        rec_system_services.push(svc);
                    }
                }
                _ => {}
            }
        }
        rec_crds.sort();
        rec_system_services.sort();
    }
    Ok((rec_crds, rec_system_services))
}

// ── Job helpers ───────────────────────────────────────────────────────────────

/// Deletes a Job using foreground deletion and waits until it disappears.
pub async fn delete_job_and_wait(job_api: &Api<Job>, job_name: &str) -> Result<()> {
    match job_api.delete(job_name, &DeleteParams::foreground()).await {
        Ok(eith) => {
            if let either::Left(j) = eith {
                let uid = j.metadata.uid.unwrap_or_default();
                let cond = await_condition(job_api.clone(), job_name, conditions::is_deleted(&uid));
                tokio::time::timeout(std::time::Duration::from_secs(20), cond)
                    .await
                    .map_err(Error::Elapsed)?
                    .map_err(Error::KubeWaitError)?;
            }
        }
        Err(e) => tracing::warn!("Deleting Job {} failed with: {e}", job_name),
    }
    Ok(())
}

/// Applies (SSA patch) a Job definition, falling back to delete-then-create
/// if the server-side apply is rejected.
pub async fn upsert_job(job_api: &Api<Job>, job_name: &str, job_def: Value) -> Result<()> {
    match job_api
        .patch(
            job_name,
            &PatchParams::apply(&get_client_name()).force(),
            &Patch::Apply(job_def.clone()),
        )
        .await
    {
        Ok(_) => Ok(()),
        Err(_) => {
            if let either::Left(j) = job_api
                .delete(job_name, &DeleteParams::foreground())
                .await
                .map_err(Error::KubeError)?
            {
                let uid = j.metadata.uid.unwrap_or_default();
                let cond = await_condition(job_api.clone(), job_name, conditions::is_deleted(&uid));
                tokio::time::timeout(std::time::Duration::from_secs(20), cond)
                    .await
                    .map_err(Error::Elapsed)?
                    .map_err(Error::KubeWaitError)?;
            }
            job_api
                .create(
                    &PostParams::default(),
                    &serde_json::from_value(job_def).map_err(Error::SerializationError)?,
                )
                .await
                .map_err(Error::KubeError)?;
            Ok(())
        }
    }
}

// ── Generic entry point (finalizer wrapper) ───────────────────────────────────

/// Entry point called by the kube controller. Wires tracing, metrics, and the
/// finalizer, then delegates to the `Reconciler` impl on `T`.
pub async fn run_with_finalizer<T>(inst: Arc<T>, ctx: Arc<Context>) -> Result<Action>
where
    T: InstanceKind + Reconciler,
{
    let trace_id = telemetry::get_trace_id();
    Span::current().record("trace_id", field::display(&trace_id));
    if trace_id != opentelemetry::trace::TraceId::INVALID {
        Span::current().record("trace_id", field::display(&trace_id));
    }
    let _mes = inst.count_and_measure_metrics(&ctx, &trace_id);
    ctx.diagnostics.write().await.last_event = Utc::now();
    let ns = inst.namespace().unwrap_or_default();
    let insts: Api<T> = Api::namespaced(ctx.client.clone(), &ns);

    finalizer(&insts, T::finalizer_name(), inst, |event| async {
        match event {
            Finalizer::Apply(inst) => inst.reconcile(ctx.clone()).await,
            Finalizer::Cleanup(inst) => inst.cleanup(ctx.clone()).await,
        }
    })
    .await
    .map_err(|e| Error::FinalizerError(Box::new(e)))
}

// ── Generic reconcile (Apply) ─────────────────────────────────────────────────

pub async fn do_reconcile<T: InstanceKind>(inst: &T, ctx: Arc<Context>) -> Result<Action> {
    tracing::debug!(
        "Reconcilling {}Instance {}/{}",
        T::type_name(),
        inst.namespace().unwrap(),
        inst.name_any()
    );
    ctx.diagnostics.write().await.last_event = Utc::now();
    let mut hbs = ctx.renderer.clone();
    let client = ctx.client.clone();
    let my_ns = ctx.client.default_namespace();
    let ns = inst.namespace().unwrap();
    let job_name = format!("{}--{}--{}", T::type_name(), ns, inst.name_any());
    let current_version = inst.current_tag();

    let mut context = ctx.base_context.clone();
    {
        let obj = context.as_object_mut().unwrap();
        obj.insert("name".to_string(), inst.name_any().into());
        obj.insert("namespace".to_string(), ns.clone().into());
        obj.insert("package_type".to_string(), T::type_name().into());
        obj.insert("package_action".to_string(), "install".into());
        obj.insert("job_name".to_string(), job_name.clone().into());
        obj.insert("digest".to_string(), inst.clone().get_options_digest().into());
        obj.insert("oci_mount".to_string(), false.into());
    }

    // ── Package lookup ────────────────────────────────────────────────────
    let (pck, pull_secret) = {
        let packages = ctx.packages.read().await;
        let jukebox = inst.spec_jukebox();
        if !packages.keys().any(|x| x == jukebox) {
            drop(packages);
            inst.clone()
                .set_missing_box(inst.spec_jukebox().to_string())
                .await?;
            return Ok(Action::requeue(Duration::from_secs(15 * 60)));
        }
        let pck = packages[jukebox]
            .packages
            .iter()
            .find(|p| {
                p.metadata.name == inst.spec_package()
                    && p.metadata.category == inst.spec_category()
                    && p.metadata.usage == T::package_type()
                    && p.is_min_version_ok(current_version.clone())
                    && p.is_vynil_version_ok()
            })
            .cloned();
        let pull_secret = packages[jukebox].pull_secret.clone();
        (pck, pull_secret)
        // packages lock released here
    };

    let pck = match pck {
        Some(p) => p,
        None => {
            inst.clone()
                .set_missing_package(inst.spec_category().to_string(), inst.spec_package().to_string())
                .await?;
            return Ok(Action::requeue(Duration::from_secs(15 * 60)));
        }
    };

    // ── Pull secret ───────────────────────────────────────────────────────
    if let Some(ps) = pull_secret {
        let obj = context.as_object_mut().unwrap();
        obj.insert("use_secret".to_string(), true.into());
        obj.insert("pull_secret".to_string(), ps.into());
    } else {
        context
            .as_object_mut()
            .unwrap()
            .insert("use_secret".to_string(), false.into());
    }

    {
        let obj = context.as_object_mut().unwrap();
        obj.insert("tag".to_string(), pck.tag.clone().into());
        obj.insert("image".to_string(), pck.image.clone().into());
        obj.insert("registry".to_string(), pck.registry.clone().into());
    }

    // ── Requirements ──────────────────────────────────────────────────────
    if let Some(action) = inst.check_requirements(pck.requirements, client.clone()).await? {
        return Ok(action);
    }

    // ── Recommendations ───────────────────────────────────────────────────
    let recos = inst
        .build_recommendations(pck.recommandations, client.clone())
        .await?;
    {
        let obj = context.as_object_mut().unwrap();
        obj.insert("rec_crds".to_string(), recos.crds.join(",").into());
        obj.insert(
            "rec_system_services".to_string(),
            recos.system_services.join(",").into(),
        );
        obj.insert(
            "rec_tenant_services".to_string(),
            recos.tenant_services.join(",").into(),
        );
    }

    // ── Value script ──────────────────────────────────────────────────────
    if let Some(value_script) = pck.value_script {
        let mut rhai = Script::new(vec![]);
        inst.set_rhai_instance(&mut rhai);
        let script = serde_json::from_str::<String>(&value_script).map_err(Error::JsonError)?;
        let val = rhai.eval_map_string(&script)?;
        context
            .as_object_mut()
            .unwrap()
            .insert("ctrl_values".to_string(), format!("{:?}", val).into());
    } else {
        context
            .as_object_mut()
            .unwrap()
            .insert("ctrl_values".to_string(), "\"{}\"".into());
    }

    // ── Force-reinstall annotation ────────────────────────────────────────
    let job_api: Api<Job> = Api::namespaced(client.clone(), my_ns);
    if inst
        .annotations()
        .contains_key("vynil.solidite.fr/force-reinstall")
    {
        let api = Api::<T>::namespaced(client.clone(), &inst.namespace().unwrap());
        let patch = Patch::Json::<()>(
            serde_json::from_value(serde_json::json!([
                {"op": "remove", "path": "/metadata/annotations/vynil.solidite.fr~1force-reinstall"}
            ]))
            .unwrap(),
        );
        api.patch(&inst.name_any(), &PatchParams::default(), &patch)
            .await
            .map_err(Error::KubeError)?;
        let job = job_api.get_metadata_opt(&job_name).await;
        if job.is_ok() && job.unwrap().is_some() {
            delete_job_and_wait(&job_api, &job_name).await?;
        }
    }

    // ── Create/update the install job ─────────────────────────────────────
    let job_def_str = hbs.render("{{> package.yaml }}", &context)?;
    let job_def: Value = common::yamlhandler::yaml_str_to_json(&job_def_str)?;
    upsert_job(&job_api, &job_name, job_def).await?;

    tracing::debug!(
        "Reconcilling {}Instance {}/{} Done",
        T::type_name(),
        inst.namespace().unwrap(),
        inst.name_any()
    );
    Ok(Action::requeue(Duration::from_secs(15 * 60)))
}

// ── Generic cleanup (Cleanup / finalizer deletion) ────────────────────────────

pub async fn do_cleanup<T: InstanceKind>(inst: &T, ctx: Arc<Context>) -> Result<Action> {
    ctx.diagnostics.write().await.last_event = Utc::now();
    let mut hbs = ctx.renderer.clone();
    let client = ctx.client.clone();
    let my_ns = ctx.client.default_namespace();
    let ns = inst.namespace().unwrap();
    let job_name = format!("{}--{}--{}", T::type_name(), ns, inst.name_any());
    let current_version = inst.current_tag();

    let mut context = ctx.base_context.clone();
    {
        let obj = context.as_object_mut().unwrap();
        obj.insert("name".to_string(), inst.name_any().into());
        obj.insert("namespace".to_string(), ns.clone().into());
        obj.insert("package_type".to_string(), T::type_name().into());
        obj.insert("package_action".to_string(), "delete".into());
        obj.insert("job_name".to_string(), job_name.clone().into());
        obj.insert("digest".to_string(), inst.clone().get_options_digest().into());
        obj.insert("oci_mount".to_string(), false.into());
    }

    // ── Package lookup ────────────────────────────────────────────────────
    let (pck, pull_secret) = {
        let packages = ctx.packages.read().await;
        let jukebox = inst.spec_jukebox();
        if !packages.keys().any(|x| x == jukebox) {
            return Ok(Action::await_change());
        }
        let pck = packages[jukebox]
            .packages
            .iter()
            .find(|p| {
                p.metadata.name == inst.spec_package()
                    && p.metadata.category == inst.spec_category()
                    && p.metadata.usage == T::package_type()
                    && p.is_min_version_ok(current_version.clone())
                    && p.is_vynil_version_ok()
            })
            .cloned();
        let pull_secret = packages[jukebox].pull_secret.clone();
        (pck, pull_secret)
        // packages lock released here
    };

    let pck = match pck {
        Some(p) => p,
        None => {
            if inst.have_child() {
                return Err(Error::Other(String::from(
                    "This install have child but the package cannot be found",
                )));
            }
            return Ok(Action::await_change());
        }
    };

    // ── Pull secret ───────────────────────────────────────────────────────
    if let Some(ps) = pull_secret {
        let obj = context.as_object_mut().unwrap();
        obj.insert("use_secret".to_string(), true.into());
        obj.insert("pull_secret".to_string(), ps.into());
    } else {
        context
            .as_object_mut()
            .unwrap()
            .insert("use_secret".to_string(), false.into());
    }

    {
        let obj = context.as_object_mut().unwrap();
        obj.insert("tag".to_string(), pck.tag.clone().into());
        obj.insert("image".to_string(), pck.image.clone().into());
        obj.insert("registry".to_string(), pck.registry.clone().into());
        // recommendations are not needed for cleanup
        obj.insert("rec_crds".to_string(), "".into());
        obj.insert("rec_system_services".to_string(), "".into());
        obj.insert("rec_tenant_services".to_string(), "".into());
    }

    // ── Value script ──────────────────────────────────────────────────────
    if let Some(value_script) = pck.value_script {
        let mut rhai = Script::new(vec![]);
        inst.set_rhai_instance(&mut rhai);
        let script = serde_json::from_str::<String>(&value_script).map_err(Error::JsonError)?;
        let val = rhai.eval_map_string(&script)?;
        context
            .as_object_mut()
            .unwrap()
            .insert("ctrl_values".to_string(), format!("{:?}", val).into());
    } else {
        context
            .as_object_mut()
            .unwrap()
            .insert("ctrl_values".to_string(), "\"{}\"".into());
    }

    // ── Delete the install job ────────────────────────────────────────────
    let job_api: Api<Job> = Api::namespaced(client.clone(), my_ns);
    let job = job_api.get_metadata_opt(&job_name).await;
    if job.is_ok() && job.unwrap().is_some() {
        delete_job_and_wait(&job_api, &job_name).await?;
    }

    // ── Create and run the delete job ─────────────────────────────────────
    tracing::info!("Deleting with: {:?}", &context);
    let job_def_str = hbs.render("{{> package.yaml }}", &context)?;
    let job_def: Value = common::yamlhandler::yaml_str_to_json(&job_def_str)?;
    job_api
        .create(
            &PostParams::default(),
            &serde_json::from_value(job_def).map_err(Error::SerializationError)?,
        )
        .await
        .map_err(Error::KubeError)?;

    // Wait for the delete job to complete
    let cond = await_condition(job_api.clone(), &job_name, conditions::is_job_completed());
    tokio::time::timeout(std::time::Duration::from_secs(10 * 60), cond)
        .await
        .map_err(Error::Elapsed)?
        .map_err(Error::KubeWaitError)?;

    // Delete the delete job
    match job_api.delete(&job_name, &DeleteParams::foreground()).await {
        Ok(_) => {}
        Err(e) => tracing::warn!("Deleting Job {} failed with: {e}", &job_name),
    }
    Ok(Action::await_change())
}
