use crate::{Error, Reconciler, Result, SystemInstance, get_client_name, manager::Context, telemetry};
use async_trait::async_trait;
use chrono::Utc;
use common::{
    rhaihandler::Script,
    vynilpackage::{VynilPackageRecommandation, VynilPackageType},
};
use k8s_openapi::{
    api::batch::v1::Job, apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition,
};
use kube::{
    api::{Api, DeleteParams, Patch, PatchParams, PostParams, ResourceExt},
    runtime::{
        conditions,
        controller::Action,
        finalizer::{Event as Finalizer, finalizer},
        wait::await_condition,
    },
};
use serde_json::Value;
use std::sync::Arc;
use tokio::time::Duration;
use tracing::{Span, field, instrument};
static SYSTEM_FINALIZER: &str = "systeminstances.vynil.solidite.fr";


#[instrument(skip(ctx, inst), fields(trace_id))]
pub async fn reconcile(inst: Arc<SystemInstance>, ctx: Arc<Context>) -> Result<Action> {
    let trace_id = telemetry::get_trace_id();
    Span::current().record("trace_id", field::display(&trace_id));
    if trace_id != opentelemetry::trace::TraceId::INVALID {
        Span::current().record("trace_id", field::display(&trace_id));
    }
    let _mes = ctx.metrics.system_instance.count_and_measure(&inst, &trace_id);
    ctx.diagnostics.write().await.last_event = Utc::now();
    let ns = inst.namespace().unwrap_or_default(); // inst is namespace scoped
    let insts: Api<SystemInstance> = Api::namespaced(ctx.client.clone(), &ns);

    finalizer(&insts, SYSTEM_FINALIZER, inst, |event| async {
        match event {
            Finalizer::Apply(inst) => inst.reconcile(ctx.clone()).await,
            Finalizer::Cleanup(inst) => inst.cleanup(ctx.clone()).await,
        }
    })
    .await
    .map_err(|e| Error::FinalizerError(Box::new(e)))
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
        context
            .as_object_mut()
            .unwrap()
            .insert("name".to_string(), self.name_any().into());
        context
            .as_object_mut()
            .unwrap()
            .insert("namespace".to_string(), ns.clone().into());
        context
            .as_object_mut()
            .unwrap()
            .insert("package_type".to_string(), "system".into());
        context
            .as_object_mut()
            .unwrap()
            .insert("package_action".to_string(), "install".into());
        context
            .as_object_mut()
            .unwrap()
            .insert("job_name".to_string(), job_name.clone().into());
        context
            .as_object_mut()
            .unwrap()
            .insert("digest".to_string(), self.clone().get_options_digest().into());
        let current_version = if let Some(status) = self.status.clone() {
            if let Some(v) = status.tag {
                v
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        let packages = ctx.packages.read().await;
        if !packages.keys().any(|x| *x == self.spec.jukebox) {
            self.clone().set_missing_box(self.spec.jukebox.clone()).await?;
            return Ok(Action::requeue(Duration::from_secs(15 * 60)));
        } else if !packages[&self.spec.jukebox]
            .packages
            .clone()
            .into_iter()
            .any(|p| {
                p.metadata.name == self.spec.package
                    && p.metadata.category == self.spec.category
                    && p.metadata.usage == VynilPackageType::System
                    && (p.is_min_version_ok(current_version.clone()))
            })
        {
            self.clone()
                .set_missing_package(self.spec.category.clone(), self.spec.package.clone())
                .await?;
            return Ok(Action::requeue(Duration::from_secs(15 * 60)));
        }
        if let Some(pull_secret) = packages[&self.spec.jukebox].pull_secret.clone() {
            context
                .as_object_mut()
                .unwrap()
                .insert("use_secret".to_string(), true.into());
            context
                .as_object_mut()
                .unwrap()
                .insert("pull_secret".to_string(), pull_secret.into());
        } else {
            context
                .as_object_mut()
                .unwrap()
                .insert("use_secret".to_string(), false.into());
        }
        let pck = packages[&self.spec.jukebox]
            .packages
            .clone()
            .into_iter()
            .find(|p| {
                p.metadata.name == self.spec.package
                    && p.metadata.category == self.spec.category
                    && p.metadata.usage == VynilPackageType::System
                    && (p.is_min_version_ok(current_version.clone()))
            })
            .unwrap();
        context
            .as_object_mut()
            .unwrap()
            .insert("tag".to_string(), pck.tag.into());
        context
            .as_object_mut()
            .unwrap()
            .insert("image".to_string(), pck.image.into());
        context
            .as_object_mut()
            .unwrap()
            .insert("registry".to_string(), pck.registry.into());
        // Check requierements
        for req in pck.requirements {
            let (res, mes, requeue) = req.check_system(self, client.clone()).await?;
            if !res {
                self.clone().set_missing_requirement(mes).await?;
                return Ok(Action::requeue(Duration::from_secs(requeue)));
            }
        }
        // Build recommandations strings
        let mut rec_crds: Vec<String> = Vec::new();
        let mut rec_system_services: Vec<String> = Vec::new();
        let current_system_services =
            common::instanceservice::ServiceInstance::get_all_services_names().await?;
        if let Some(recos) = pck.recommandations {
            for reco in recos {
                match reco {
                    VynilPackageRecommandation::CustomResourceDefinition(crd) => {
                        let api: Api<CustomResourceDefinition> = Api::all(client.clone());
                        let r = api.get_metadata_opt(&crd).await.map_err(Error::KubeError)?;
                        if r.is_some() {
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
        context
            .as_object_mut()
            .unwrap()
            .insert("rec_crds".to_string(), rec_crds.join(",").into());
        context
            .as_object_mut()
            .unwrap()
            .insert("rec_tenant_services".to_string(), "".into());
        context.as_object_mut().unwrap().insert(
            "rec_system_services".to_string(),
            rec_system_services.join(",").into(),
        );

        // Compute the controller values
        if pck.value_script.is_some() {
            let mut rhai = Script::new(vec![]);
            rhai.ctx.set_value("instance", self.clone());
            let value_script = pck.value_script.unwrap();
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
        // Evrything is good to go

        // Support job deletion annotation to force reinstall
        let job_api: Api<Job> = Api::namespaced(client.clone(), my_ns);
        if self
            .annotations()
            .contains_key("vynil.solidite.fr/force-reinstall")
        {
            // remove the annotation
            let stms = Api::<SystemInstance>::namespaced(client.clone(), &self.namespace().unwrap());
            let pp = PatchParams::default();
            let patch = Patch::Json::<()>(
                serde_json::from_value(serde_json::json!([
                    {"op": "remove", "path": "/metadata/annotations/vynil.solidite.fr~1force-reinstall"}
                ]))
                .unwrap(),
            );
            let _patched = stms
                .patch(&self.name_any(), &pp, &patch)
                .await
                .map_err(Error::KubeError)?;
            // delete the job if exist
            let job = job_api.get_metadata_opt(&job_name).await;
            if job.is_ok() && job.unwrap().is_some() {
                match job_api.delete(&job_name, &DeleteParams::foreground()).await {
                    Ok(eith) => {
                        if let either::Left(j) = eith {
                            let uid = j.metadata.uid.unwrap_or_default();
                            let cond =
                                await_condition(job_api.clone(), &job_name, conditions::is_deleted(&uid));
                            tokio::time::timeout(std::time::Duration::from_secs(20), cond)
                                .await
                                .map_err(Error::Elapsed)?
                                .map_err(Error::KubeWaitError)?;
                        }
                    }
                    Err(e) => tracing::warn!("Deleting Job {} failed with: {e}", &job_name),
                };
            }
        }

        // Create the job
        //tracing::info!("Creating with: {:?}", &context);
        let job_def_str = hbs.render("{{> package.yaml }}", &context)?;
        let job_def: Value = serde_yaml::from_str(&job_def_str).map_err(Error::YamlError)?;
        let _job = match job_api
            .patch(
                &job_name,
                &PatchParams::apply(&get_client_name()).force(),
                &Patch::Apply(job_def.clone()),
            )
            .await
        {
            Ok(j) => j,
            Err(_) => {
                if let either::Left(j) = job_api
                    .delete(&job_name, &DeleteParams::foreground())
                    .await
                    .map_err(Error::KubeError)?
                {
                    let uid = j.metadata.uid.unwrap_or_default();
                    let cond = await_condition(job_api.clone(), &job_name, conditions::is_deleted(&uid));
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
                    .map_err(Error::KubeError)?
            }
        };
        // Wait for the Job completion
        /*let cond = await_condition(job_api.clone(), &job_name, conditions::is_job_completed());
        tokio::time::timeout(std::time::Duration::from_secs(10 * 60), cond)
            .await
            .map_err(Error::Elapsed)?
            .map_err(Error::KubeWaitError)?;*/
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
        context
            .as_object_mut()
            .unwrap()
            .insert("name".to_string(), self.name_any().into());
        context
            .as_object_mut()
            .unwrap()
            .insert("namespace".to_string(), ns.clone().into());
        context
            .as_object_mut()
            .unwrap()
            .insert("package_type".to_string(), "system".into());
        context
            .as_object_mut()
            .unwrap()
            .insert("package_action".to_string(), "delete".into());
        context
            .as_object_mut()
            .unwrap()
            .insert("job_name".to_string(), job_name.clone().into());
        context
            .as_object_mut()
            .unwrap()
            .insert("digest".to_string(), self.clone().get_options_digest().into());
        let packages = ctx.packages.read().await;
        if !packages.keys().any(|x| *x == self.spec.jukebox) {
            // JukeBox doesnt exist, cannot have been installed
            return Ok(Action::await_change());
        } else if !packages[&self.spec.jukebox]
            .packages
            .clone()
            .into_iter()
            .any(|p| {
                p.metadata.name == self.spec.package
                    && p.metadata.category == self.spec.category
                    && p.metadata.usage == VynilPackageType::System
            })
        {
            // Package doesnt exist
            if self.have_child() {
                return Err(Error::Other(String::from(
                    "This install have child but the package cannot be found",
                )));
            }
            return Ok(Action::await_change());
        }
        if let Some(pull_secret) = packages[&self.spec.jukebox].pull_secret.clone() {
            context
                .as_object_mut()
                .unwrap()
                .insert("use_secret".to_string(), true.into());
            context
                .as_object_mut()
                .unwrap()
                .insert("pull_secret".to_string(), pull_secret.into());
        } else {
            context
                .as_object_mut()
                .unwrap()
                .insert("use_secret".to_string(), false.into());
        }
        let pck = packages[&self.spec.jukebox]
            .packages
            .clone()
            .into_iter()
            .find(|p| {
                p.metadata.name == self.spec.package
                    && p.metadata.category == self.spec.category
                    && p.metadata.usage == VynilPackageType::System
            })
            .unwrap();
        context
            .as_object_mut()
            .unwrap()
            .insert("tag".to_string(), pck.tag.into());
        context
            .as_object_mut()
            .unwrap()
            .insert("image".to_string(), pck.image.into());
        context
            .as_object_mut()
            .unwrap()
            .insert("registry".to_string(), pck.registry.into());
        context
            .as_object_mut()
            .unwrap()
            .insert("rec_crds".to_string(), "".into());
        context
            .as_object_mut()
            .unwrap()
            .insert("rec_tenant_services".to_string(), "".into());
        context
            .as_object_mut()
            .unwrap()
            .insert("rec_system_services".to_string(),"".into());
        // Compute the controller values
        if pck.value_script.is_some() {
            let mut rhai = Script::new(vec![]);
            rhai.ctx.set_value("instance", self.clone());
            let value_script = pck.value_script.unwrap();
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
        // Delete the install Job
        let job_api: Api<Job> = Api::namespaced(client.clone(), my_ns);
        let job = job_api.get_metadata_opt(&job_name).await;
        if job.is_ok() && job.unwrap().is_some() {
            match job_api.delete(&job_name, &DeleteParams::foreground()).await {
                Ok(eith) => {
                    if let either::Left(j) = eith {
                        let uid = j.metadata.uid.unwrap_or_default();
                        let cond = await_condition(job_api.clone(), &job_name, conditions::is_deleted(&uid));
                        tokio::time::timeout(std::time::Duration::from_secs(20), cond)
                            .await
                            .map_err(Error::Elapsed)?
                            .map_err(Error::KubeWaitError)?;
                    }
                }
                Err(e) => tracing::warn!("Deleting Job {} failed with: {e}", &job_name),
            };
        }
        // Create the delete Job
        tracing::info!("Deleting with: {:?}", &context);
        let job_def_str = hbs.render("{{> package.yaml }}", &context)?;
        let job_def: Value = serde_yaml::from_str(&job_def_str).map_err(Error::YamlError)?;
        job_api
            .create(
                &PostParams::default(),
                &serde_json::from_value(job_def).map_err(Error::SerializationError)?,
            )
            .await
            .map_err(Error::KubeError)?;

        // Wait for the Job completion
        let cond = await_condition(job_api.clone(), &job_name, conditions::is_job_completed());
        tokio::time::timeout(std::time::Duration::from_secs(10 * 60), cond)
            .await
            .map_err(Error::Elapsed)?
            .map_err(Error::KubeWaitError)?;
        // Delete the delete Job
        match job_api.delete(&job_name, &DeleteParams::foreground()).await {
            Ok(_) => {}
            Err(e) => tracing::warn!("Deleting Job {} failed with: {e}", &job_name),
        };
        Ok(Action::await_change())
    }
}

#[must_use]
pub fn error_policy(inst: Arc<SystemInstance>, error: &Error, ctx: Arc<Context>) -> Action {
    tracing::warn!(
        "reconcile failed for SystemInstance '{:?}.{:?}': {:?}",
        inst.metadata.namespace,
        inst.metadata.name,
        error
    );
    ctx.metrics.system_instance.reconcile_failure(&inst, error);
    Action::requeue(Duration::from_secs(5 * 60))
}
