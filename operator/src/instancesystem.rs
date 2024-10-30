use crate::{manager::Context, telemetry, Error, Result, Reconciler, SystemInstance};
use chrono::Utc;
use common::{get_client_name, vynilpackage::VynilPackageType};
use k8s_openapi::api::batch::v1::Job;
use kube::{
    api::{Api, DeleteParams, Patch, PatchParams, PostParams, ResourceExt},
    runtime::{
        conditions, controller::Action, finalizer::{finalizer, Event as Finalizer}, wait::await_condition
    },
};
use serde_json::Value;
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
        let mut hbs = ctx.renderer.clone();
        let client = ctx.client.clone();
        let my_ns = ctx.client.default_namespace();
        let ns = self.namespace().unwrap();
        // Find the package and set render context
        let job_name = format!("system--{}--{}", ns, self.name_any());
        let mut context = ctx.base_context.clone();
        context.as_object_mut().unwrap().insert("name".to_string(), self.name_any().into());
        context.as_object_mut().unwrap().insert("namespace".to_string(), ns.clone().into());
        context.as_object_mut().unwrap().insert("package_type".to_string(), "system".into());
        context.as_object_mut().unwrap().insert("package_action".to_string(), "install".into());
        context.as_object_mut().unwrap().insert("job_name".to_string(), job_name.clone().into());
        context.as_object_mut().unwrap().insert("digest".to_string(), self.clone().get_options_digest().into());
        let packages = ctx.packages.read().await;
        if ! packages.keys().into_iter().any(|x| *x==self.spec.jukebox) {
            self.clone().set_missing_box(self.spec.jukebox.clone()).await?;
            return Ok(Action::requeue(Duration::from_secs(15 * 60)));
        } else if ! packages[&self.spec.jukebox].packages.clone().into_iter().any(|p| p.metadata.name == self.spec.package && p.metadata.category == self.spec.category && p.metadata.usage == VynilPackageType::System) {
            self.clone().set_missing_package(self.spec.category.clone(), self.spec.package.clone()).await?;
            return Ok(Action::requeue(Duration::from_secs(15 * 60)));
        }
        if let Some(pull_secret) = packages[&self.spec.jukebox].pull_secret.clone() {
            context.as_object_mut().unwrap().insert("use_secret".to_string(), true.into());
            context.as_object_mut().unwrap().insert("pull_secret".to_string(), pull_secret.into());
        } else {
            context.as_object_mut().unwrap().insert("use_secret".to_string(), false.into());
        }
        let pck = packages[&self.spec.jukebox].packages.clone().into_iter().find(|p| p.metadata.name == self.spec.package && p.metadata.category == self.spec.category && p.metadata.usage == VynilPackageType::System).unwrap();
        context.as_object_mut().unwrap().insert("tag".to_string(), pck.tag.into());
        context.as_object_mut().unwrap().insert("image".to_string(), pck.image.into());
        context.as_object_mut().unwrap().insert("registry".to_string(), pck.registry.into());
        // Check requierements
        for req in pck.requirements {
            let (res, mes) = req.check_system(self, client.clone()).await?;
            if ! res {
                self.clone().set_missing_requirement(mes).await?;
                return Ok(Action::requeue(Duration::from_secs(15 * 60)));
            }
        }
        // Evrything is good to go
        // Create the job
        let job_def_str = hbs.render("{{> package.yaml }}", &context)?;
        let job_def: Value = serde_yaml::from_str(&job_def_str).map_err(|e| Error::YamlError(e))?;
        let job_api: Api<Job> = Api::namespaced(client.clone(), &my_ns);
        let _job = match job_api.patch(&job_name, &PatchParams::apply(&get_client_name()).force(), &Patch::Apply(job_def.clone())).await {
            Ok(j) => j,
            Err(_) => {
                if let either::Left(j) = job_api.delete(&job_name, &DeleteParams::foreground()).await.map_err(|e| Error::KubeError(e))? {
                    let uid = j.metadata.uid.unwrap_or_default();
                    let cond = await_condition(job_api.clone(), &job_name, conditions::is_deleted(&uid));
                    tokio::time::timeout(std::time::Duration::from_secs(20), cond).await.map_err(|e| Error::Elapsed(e))?.map_err(|e| Error::KubeWaitError(e))?;
                }
                job_api.create(&PostParams::default(), &serde_json::from_value(job_def).map_err(|e|Error::SerializationError(e))?).await.map_err(|e| Error::KubeError(e))?
            }
        };
        // Wait for the Job completion
        let cond = await_condition(job_api.clone(), &job_name, conditions::is_job_completed());
        tokio::time::timeout(std::time::Duration::from_secs(10*60), cond).await.map_err(|e| Error::Elapsed(e))?.map_err(|e|Error::KubeWaitError(e))?;
        Ok(Action::requeue(Duration::from_secs(15 * 60)))
    }

    // Reconcile with finalize cleanup (the object was deleted)
    async fn cleanup(&self, ctx: Arc<Context>) -> Result<Action> {
        ctx.diagnostics.write().await.last_event = Utc::now();
        let mut hbs = ctx.renderer.clone();
        let client = ctx.client.clone();
        let my_ns = ctx.client.default_namespace();
        let ns = self.namespace().unwrap();
        // Find the package and set render context
        let job_name = format!("system--{}--{}", ns, self.name_any());
        let mut context = ctx.base_context.clone();
        context.as_object_mut().unwrap().insert("name".to_string(), self.name_any().into());
        context.as_object_mut().unwrap().insert("namespace".to_string(), ns.clone().into());
        context.as_object_mut().unwrap().insert("package_type".to_string(), "system".into());
        context.as_object_mut().unwrap().insert("package_action".to_string(), "delete".into());
        context.as_object_mut().unwrap().insert("job_name".to_string(), job_name.clone().into());
        context.as_object_mut().unwrap().insert("digest".to_string(), self.clone().get_options_digest().into());
        let packages = ctx.packages.read().await;
        if ! packages.keys().into_iter().any(|x| *x==self.spec.jukebox) {
            // JukeBox doesnt exist, cannot have been installed
            return Ok(Action::await_change());
        } else if ! packages[&self.spec.jukebox].packages.clone().into_iter().any(|p| p.metadata.name == self.spec.package && p.metadata.category == self.spec.category && p.metadata.usage == VynilPackageType::System) {
            // Package doesnt exist, cannot have been installed
            return Ok(Action::await_change());
        }
        if let Some(pull_secret) = packages[&self.spec.jukebox].pull_secret.clone() {
            context.as_object_mut().unwrap().insert("use_secret".to_string(), true.into());
            context.as_object_mut().unwrap().insert("pull_secret".to_string(), pull_secret.into());
        } else {
            context.as_object_mut().unwrap().insert("use_secret".to_string(), false.into());
        }
        let pck = packages[&self.spec.jukebox].packages.clone().into_iter().find(|p| p.metadata.name == self.spec.package && p.metadata.category == self.spec.category && p.metadata.usage == VynilPackageType::System).unwrap();
        context.as_object_mut().unwrap().insert("tag".to_string(), pck.tag.into());
        context.as_object_mut().unwrap().insert("image".to_string(), pck.image.into());
        context.as_object_mut().unwrap().insert("registry".to_string(), pck.registry.into());
        // Delete the install Job
        let job_api: Api<Job> = Api::namespaced(client.clone(), &my_ns);
        let job = job_api.get_metadata_opt(&job_name).await;
        if !job.is_err() && job.unwrap().is_some() {
            match job_api.delete(&job_name, &DeleteParams::foreground()).await {
                Ok(eith) => {
                    if let either::Left(j) = eith {
                        let uid = j.metadata.uid.unwrap_or_default();
                        let cond = await_condition(job_api.clone(), &job_name, conditions::is_deleted(&uid));
                        tokio::time::timeout(std::time::Duration::from_secs(20), cond).await.map_err(|e| Error::Elapsed(e))?.map_err(|e| Error::KubeWaitError(e))?;
                    }
                },
                Err(e) => tracing::warn!("Deleting Job {} failed with: {e}", &job_name),
            };
        }
        // Create the delete Job
        let job_def_str = hbs.render("{{> package.yaml }}", &context)?;
        let job_def: Value = serde_yaml::from_str(&job_def_str).map_err(|e| Error::YamlError(e))?;
        job_api.create(&PostParams::default(), &serde_json::from_value(job_def).map_err(|e|Error::SerializationError(e))?).await.map_err(|e| Error::KubeError(e))?;

        // Wait for the Job completion
        let cond = await_condition(job_api.clone(), &job_name, conditions::is_job_completed());
        tokio::time::timeout(std::time::Duration::from_secs(10*60), cond).await.map_err(|e| Error::Elapsed(e))?.map_err(|e|Error::KubeWaitError(e))?;
        // Delete the delete Job
        match job_api.delete(&job_name, &DeleteParams::foreground()).await {
            Ok(_) => {},
            Err(e) => tracing::warn!("Deleting Job {} failed with: {e}", &job_name),
        };
        Ok(Action::await_change())
    }
}

#[must_use] pub fn error_policy(inst: Arc<SystemInstance>, error: &Error, ctx: Arc<Context>) -> Action {
    warn!("reconcile failed for '{:?}.{:?}': {:?}", inst.metadata.namespace, inst.metadata.name, error);
    ctx.metrics.system_reconcile_failure(&inst, error);
    Action::requeue(Duration::from_secs(5 * 60))
}
