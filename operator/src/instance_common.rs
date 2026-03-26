use crate::{
    Error, Reconciler, Result, get_client_name, manager::Context, metrics::ReconcileMeasurerInstance,
    telemetry,
};
use async_trait::async_trait;
use chrono::Utc;
use common::{
    rhaihandler::Script,
    vynilpackage::{VynilPackage, VynilPackageRecommandation, VynilPackageRequirement, VynilPackageType},
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
    /// Returns the version requested for initial restore, or None if absent.
    /// Default implementation returns None (SystemInstance, or no initFrom.version).
    fn init_from_version(&self) -> Option<&str> {
        None
    }
    fn have_child(&self) -> bool;
    fn get_options_digest(&mut self) -> String;

    // ── Status update methods ─────────────────────────────────────────────
    async fn set_missing_box(self, jukebox: String) -> Result<Self>;
    async fn set_missing_package(self, category: String, package: String) -> Result<Self>;
    async fn set_missing_requirement(self, reason: String) -> Result<Self>;
    /// Records that the requested init version was not found.
    /// Default no-op for instance types that don't support initFrom (e.g. SystemInstance).
    async fn set_missing_init_version(self, _version: String) -> Result<Self>
    where
        Self: Sized,
    {
        Ok(self)
    }

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

// ── Init version resolver ─────────────────────────────────────────────────────

/// Resolves the package version to use for initial restoration.
///
/// Returns `Ok(Some(version))` if a valid `initFrom.version` is found,
/// `Ok(None)` if no version was requested or the instance is already installed,
/// or `Err(Error::MissingInitVersion)` if the requested version doesn't exist.
pub async fn resolve_init_version<T: InstanceKind>(
    inst: &T,
    pck: &VynilPackage,
    cached_packages: &[VynilPackage],
    pull_secret: &Option<String>,
    client: Client,
    vynil_ns: &str,
) -> Result<Option<String>> {
    let requested = match inst.init_from_version() {
        Some(v) => v,
        None => return Ok(None),
    };
    // Already installed: ignore the init version override
    if !inst.current_tag().is_empty() {
        return Ok(None);
    }

    // 1. Check local cache first (no network call)
    let in_cache = cached_packages.iter().any(|p| {
        p.metadata.name == inst.spec_package()
            && p.metadata.category == inst.spec_category()
            && p.metadata.usage == T::package_type()
            && p.tag == requested
    });
    if in_cache {
        return Ok(Some(requested.to_string()));
    }

    // 2. Fallback: verify directly in the OCI registry
    let auth = match pull_secret {
        Some(secret_name) => {
            common::ocihandler::resolve_registry_auth(
                secret_name,
                &pck.registry,
                client,
                vynil_ns,
            )
            .await?
        }
        None => common::ocihandler::OciRegistryAuth::Anonymous,
    };
    let exists =
        common::ocihandler::verify_tag_in_registry(&pck.registry, &pck.image, requested, auth)
            .await?;
    if exists {
        Ok(Some(requested.to_string()))
    } else {
        inst.clone()
            .set_missing_init_version(requested.to_string())
            .await?;
        Err(Error::MissingInitVersion(requested.to_string()))
    }
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

    // ── Suspend annotation ────────────────────────────────────────────────
    if inst.annotations().get("vynil.solidite.fr/suspend").map(|v| v == "true").unwrap_or(false) {
        tracing::info!(
            "{}Instance {}/{} is suspended, skipping reconciliation",
            T::type_name(),
            inst.namespace().unwrap(),
            inst.name_any()
        );
        return Ok(Action::requeue(Duration::from_secs(15 * 60)));
    }

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
    let (pck, pull_secret, cached_packages) = {
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
        let cached_packages = packages[jukebox].packages.clone();
        (pck, pull_secret, cached_packages)
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
    if let Some(ref ps) = pull_secret {
        let obj = context.as_object_mut().unwrap();
        obj.insert("use_secret".to_string(), true.into());
        obj.insert("pull_secret".to_string(), ps.clone().into());
    } else {
        context
            .as_object_mut()
            .unwrap()
            .insert("use_secret".to_string(), false.into());
    }

    // ── initFrom version resolution ───────────────────────────────────────
    let effective_tag = match resolve_init_version(
        inst, &pck, &cached_packages, &pull_secret, client.clone(), my_ns,
    ).await {
        Ok(Some(v)) => v,
        Ok(None)    => pck.tag.clone(),
        Err(e)      => return Err(e),
    };
    {
        let obj = context.as_object_mut().unwrap();
        obj.insert("tag".to_string(), effective_tag.into());
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SystemInstance, TenantInstance};
    use common::{
        instancesystem::SystemInstanceSpec,
        instancetenant::{InitFrom, TenantInstanceSpec, TenantInstanceStatus},
        vynilpackage::{VynilPackage, VynilPackageMeta, VynilPackageType},
    };

    fn make_tenant(version: Option<&str>, installed_tag: Option<&str>) -> TenantInstance {
        TenantInstance {
            metadata: kube::api::ObjectMeta {
                name: Some("test".to_string()),
                namespace: Some("default".to_string()),
                ..Default::default()
            },
            spec: TenantInstanceSpec {
                jukebox: "jb".to_string(),
                category: "cat".to_string(),
                package: "pkg".to_string(),
                init_from: version.map(|v| InitFrom {
                    secret_name: None,
                    sub_path: None,
                    snapshot: "snap1".to_string(),
                    version: Some(v.to_string()),
                }),
                options: None,
            },
            status: installed_tag.map(|t| TenantInstanceStatus {
                tag: Some(t.to_string()),
                conditions: vec![],
                digest: None,
                tfstate: None,
                rhaistate: None,
                befores: None,
                vitals: None,
                scalables: None,
                others: None,
                posts: None,
                services: None,
            }),
        }
    }

    fn make_package(name: &str, category: &str, tag: &str, usage: VynilPackageType) -> VynilPackage {
        VynilPackage {
            registry: "docker.io".to_string(),
            image: "test/image".to_string(),
            tag: tag.to_string(),
            metadata: VynilPackageMeta {
                name: name.to_string(),
                category: category.to_string(),
                description: "".to_string(),
                app_version: None,
                usage,
                features: vec![],
            },
            requirements: vec![],
            recommandations: None,
            options: None,
            value_script: None,
        }
    }

    fn fake_client() -> kube::Client {
        let config = kube::Config::new("http://localhost:9999".parse().unwrap());
        kube::Client::try_from(config).unwrap()
    }

    // ── Tests init_from_version() ─────────────────────────────────────────

    #[test]
    fn test_init_from_version_tenant_with_version() {
        let inst = make_tenant(Some("1.5.0"), None);
        assert_eq!(inst.init_from_version(), Some("1.5.0"));
    }

    #[test]
    fn test_init_from_version_tenant_no_init_from() {
        let inst = make_tenant(None, None);
        assert_eq!(inst.init_from_version(), None);
    }

    #[test]
    fn test_init_from_version_system_default() {
        let inst = SystemInstance {
            metadata: kube::api::ObjectMeta::default(),
            spec: SystemInstanceSpec {
                jukebox: "jb".to_string(),
                category: "cat".to_string(),
                package: "pkg".to_string(),
                options: None,
            },
            status: None,
        };
        assert_eq!(inst.init_from_version(), None);
    }

    // ── Tests resolve_init_version() ─────────────────────────────────────

    #[tokio::test]
    async fn test_resolve_init_version_no_version() {
        let inst = make_tenant(None, None);
        let pck = make_package("pkg", "cat", "1.0.0", VynilPackageType::Tenant);
        let result = resolve_init_version(&inst, &pck, &[], &None, fake_client(), "default").await;
        assert!(matches!(result, Ok(None)));
    }

    #[tokio::test]
    async fn test_resolve_init_version_already_installed() {
        let inst = make_tenant(Some("1.5.0"), Some("2.0.0"));
        let pck = make_package("pkg", "cat", "1.0.0", VynilPackageType::Tenant);
        let result = resolve_init_version(&inst, &pck, &[], &None, fake_client(), "default").await;
        assert!(matches!(result, Ok(None)));
    }

    #[tokio::test]
    async fn test_resolve_init_version_found_in_cache() {
        let inst = make_tenant(Some("1.5.0"), None);
        let pck = make_package("pkg", "cat", "1.0.0", VynilPackageType::Tenant);
        let cached = make_package("pkg", "cat", "1.5.0", VynilPackageType::Tenant);
        let result =
            resolve_init_version(&inst, &pck, &[cached], &None, fake_client(), "default").await;
        assert!(matches!(result, Ok(Some(ref v)) if v == "1.5.0"));
    }

    #[tokio::test]
    #[ignore = "requires a real OCI registry"]
    async fn test_resolve_init_version_missing_in_registry() {
        // Covered by integration tests — requires a running OCI registry.
    }

    // ── Tests do_reconcile effective_tag selection ────────────────────────
    // These tests document the integration behavior: resolve_init_version()
    // return value determines the tag inserted into the Handlebars context.

    /// do_reconcile scenario 1: no initFrom → resolve returns None → use pck.tag
    #[tokio::test]
    async fn test_do_reconcile_tag_no_init_from() {
        let inst = make_tenant(None, None);
        let pck = make_package("pkg", "cat", "2.0.0", VynilPackageType::Tenant);
        let result = resolve_init_version(&inst, &pck, &[], &None, fake_client(), "default").await;
        assert!(matches!(result, Ok(None)));
        // Ok(None) → do_reconcile uses pck.tag ("2.0.0")
    }

    /// do_reconcile scenario 2: first install with valid version in cache → tag overridden
    #[tokio::test]
    async fn test_do_reconcile_tag_init_from_version_in_cache() {
        let inst = make_tenant(Some("1.5.0"), None);
        let pck = make_package("pkg", "cat", "2.0.0", VynilPackageType::Tenant);
        let cached = make_package("pkg", "cat", "1.5.0", VynilPackageType::Tenant);
        let result =
            resolve_init_version(&inst, &pck, &[cached], &None, fake_client(), "default").await;
        // Ok(Some("1.5.0")) → do_reconcile uses "1.5.0" instead of "2.0.0"
        assert!(matches!(result, Ok(Some(ref v)) if v == "1.5.0"));
    }

    /// do_reconcile scenario 3: version not in cache → OCI check required (ignored, needs registry)
    #[tokio::test]
    #[ignore = "requires a real OCI registry"]
    async fn test_do_reconcile_tag_init_from_version_missing() {
        // Instance with initFrom.version = "0.0.1", tag not in cache, not in registry.
        // Expected: Err(MissingInitVersion), no job created.
    }

    /// do_reconcile scenario 4: already installed (status.tag non-empty) → resolve returns None → upgrade path
    #[tokio::test]
    async fn test_do_reconcile_tag_already_installed_ignores_init_from() {
        let inst = make_tenant(Some("1.5.0"), Some("1.5.0"));
        let pck = make_package("pkg", "cat", "2.0.0", VynilPackageType::Tenant);
        let result = resolve_init_version(&inst, &pck, &[], &None, fake_client(), "default").await;
        // Ok(None) → do_reconcile uses pck.tag ("2.0.0") for normal upgrade
        assert!(matches!(result, Ok(None)));
    }
}
