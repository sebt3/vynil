use crate::{Error, JukeBox, Reconciler, Result, get_client_name, manager::Context, telemetry};
use async_trait::async_trait;
use chrono::Utc;
use k8s_openapi::api::batch::v1::{CronJob, Job};
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

static JUKEBOX_FINALIZER: &str = "jukeboxes.vynil.solidite.fr";

#[instrument(skip(ctx, dist), fields(trace_id))]
pub async fn reconcile(dist: Arc<JukeBox>, ctx: Arc<Context>) -> Result<Action> {
    let trace_id = telemetry::get_trace_id();
    Span::current().record("trace_id", field::display(&trace_id));
    if trace_id != opentelemetry::trace::TraceId::INVALID {
        Span::current().record("trace_id", field::display(&trace_id));
    }
    let _mes = ctx.metrics.jukebox.count_and_measure(&dist, &trace_id);
    ctx.diagnostics.write().await.last_event = Utc::now();
    let dists: Api<JukeBox> = Api::all(ctx.client.clone());

    finalizer(&dists, JUKEBOX_FINALIZER, dist, |event| async {
        match event {
            Finalizer::Apply(dist) => dist.reconcile(ctx.clone()).await,
            Finalizer::Cleanup(dist) => dist.cleanup(ctx.clone()).await,
        }
    })
    .await
    .map_err(|e| Error::FinalizerError(Box::new(e)))
}

#[async_trait]
impl Reconciler for JukeBox {
    // Reconcile (for non-finalizer related changes)
    async fn reconcile(&self, ctx: Arc<Context>) -> Result<Action> {
        tracing::debug!("Reconcilling JukeBox {}", self.name_any());
        ctx.diagnostics.write().await.last_event = Utc::now();
        let mut hbs = ctx.renderer.clone();
        let client = ctx.client.clone();
        let ns = ctx.client.default_namespace();
        let job_name = format!("scan-{}", self.name_any());
        let mut context = ctx.base_context.clone();
        context
            .as_object_mut()
            .unwrap()
            .insert("name".to_string(), self.name_any().into());
        context
            .as_object_mut()
            .unwrap()
            .insert("job_name".to_string(), job_name.clone().into());
        context
            .as_object_mut()
            .unwrap()
            .insert("schedule".to_string(), self.spec.schedule.clone().into());
        // Create the CronJob
        let cj_def_str = hbs.render("{{> cronscan.yaml }}", &context)?;
        let cj_def: Value = serde_yaml::from_str(&cj_def_str).map_err(Error::YamlError)?;
        let cron_api: Api<CronJob> = Api::namespaced(client.clone(), ns);
        cron_api
            .patch(
                &job_name,
                &PatchParams::apply(&get_client_name()).force(),
                &Patch::Apply(cj_def),
            )
            .await
            .map_err(Error::KubeError)?;

        // Support job deletion annotation to force reinstall
        let job_api: Api<Job> = Api::namespaced(client.clone(), ns);
        if self.annotations().contains_key("vynil.solidite.fr/force-scan") {
            // remove the annotation
            let stms = Api::<JukeBox>::all(client.clone());
            let pp = PatchParams::default();
            let patch = Patch::Json::<()>(
                serde_json::from_value(serde_json::json!([
                    {"op": "remove", "path": "/metadata/annotations/vynil.solidite.fr~1force-scan"}
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

        // Create the Job
        let job_def_str = hbs.render("{{> scan.yaml }}", &context)?;
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
        let cond = await_condition(job_api.clone(), &job_name, conditions::is_job_completed());
        tokio::time::timeout(std::time::Duration::from_secs(10 * 60), cond)
            .await
            .map_err(Error::Elapsed)?
            .map_err(Error::KubeWaitError)?;
        tracing::debug!("Reconcilling JukeBox {} Done", self.name_any());
        //tracing::info!("Updating packages cache");
        match JukeBox::list().await {
            Ok(lst) => ctx.set_package_cache(&lst).await,
            Err(e) => tracing::warn!("While listing jukebox: {:?}", e),
        };
        Ok(Action::requeue(Duration::from_secs(15 * 60)))
    }

    // Reconcile with finalize cleanup (the object was deleted)
    async fn cleanup(&self, ctx: Arc<Context>) -> Result<Action> {
        ctx.diagnostics.write().await.last_event = Utc::now();
        let client = ctx.client.clone();
        let ns = ctx.client.default_namespace();
        let job_name = format!("scan-{}", self.name_any());
        let cron_api: Api<CronJob> = Api::namespaced(client.clone(), ns);
        let cron = cron_api.get_metadata_opt(&job_name).await;
        if cron.is_ok() && cron.unwrap().is_some() {
            match cron_api.delete(&job_name, &DeleteParams::foreground()).await {
                Ok(_) => {}
                Err(e) => tracing::warn!("Deleting CronJob {} failed with: {e}", &job_name),
            };
        }
        let job_api: Api<Job> = Api::namespaced(client.clone(), ns);
        let job = job_api.get_metadata_opt(&job_name).await;
        if job.is_ok() && job.unwrap().is_some() {
            match job_api.delete(&job_name, &DeleteParams::foreground()).await {
                Ok(_) => {}
                Err(e) => tracing::warn!("Deleting Job {} failed with: {e}", &job_name),
            };
        }
        Ok(Action::await_change())
    }
}

#[must_use]
pub fn error_policy(dist: Arc<JukeBox>, error: &Error, ctx: Arc<Context>) -> Action {
    tracing::warn!(
        "reconcile failed for JukeBox {:?}: {:?}",
        dist.metadata.name,
        error
    );
    ctx.metrics.jukebox.reconcile_failure(&dist, error);
    Action::requeue(Duration::from_secs(5 * 60))
}
