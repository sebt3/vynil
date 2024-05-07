use crate::{manager::Context, telemetry, Error, Result, Reconciler, pvc::PersistentVolumeClaimHandler, jobs::JobHandler, cronjobs::CronJobHandler, events};
use chrono::Utc;
use kube::{
    api::{Api, ResourceExt, Resource},
    runtime::{
        controller::Action,
        events::Recorder,
        finalizer::{finalizer, Event as Finalizer},
    },
};
use std::sync::Arc;
use tokio::time::Duration;
use tracing::{Span, debug, field, info, instrument, warn};
use async_trait::async_trait;
pub use k8s::distrib::{Distrib,DistribStatus};

static DISTRIB_FINALIZER: &str = "distribs.vynil.solidite.fr";

#[instrument(skip(ctx, dist), fields(trace_id))]
pub async fn reconcile(dist: Arc<Distrib>, ctx: Arc<Context>) -> Result<Action> {
    let trace_id = telemetry::get_trace_id();
    Span::current().record("trace_id", &field::display(&trace_id));
    let _mes = ctx.metrics.dist_count_and_measure();
    let dists: Api<Distrib> = Api::all(ctx.client.clone());

    info!("Reconciling Distrib \"{}\"", dist.name_any());
    finalizer(&dists, DISTRIB_FINALIZER, dist, |event| async {
        match event {
            Finalizer::Apply(dist) => dist.reconcile(ctx.clone()).await,
            Finalizer::Cleanup(dist) => dist.cleanup(ctx.clone()).await,
        }
    }).await.map_err(|e| Error::FinalizerError(Box::new(e)))
}

#[async_trait]
impl Reconciler for Distrib {
    // Reconcile (for non-finalizer related changes)
    async fn reconcile(&self, ctx: Arc<Context>) -> Result<Action> {
        ctx.diagnostics.write().await.last_event = Utc::now();
        let reporter = ctx.diagnostics.read().await.reporter.clone();
        let recorder = Recorder::new(ctx.client.clone(), reporter, self.object_ref(&()));
        let name = self.name_any();
        let ns = ctx.client.default_namespace();
        let clone_name= format!("{name}-clone");
        // TODO: Validate the git url Set illegal if not legal
        /*if name == "illegal" {
            return Err(Error::IllegalInstall); // error names show up in metrics
        }*/

        let mut pvcs = PersistentVolumeClaimHandler::new(ctx.client.clone(), ns);
        if !pvcs.have(format!("{name}-distrib").as_str()).await {
            info!("Creating {name}-distrib PersistentVolumeClaim");
            // TODO: Should use controller parameters to set this
            let pvc = pvcs.create(format!("{name}-distrib").as_str(), &serde_json::json!({
                "accessModes": ["ReadWriteOnce"],
                "resources": {
                  "requests": {
                    "storage": "256Mi"
                  }
                },
                "volumeMode": "Filesystem",
                /*"storageClassName": "local-path",*/
            })).await.unwrap();
            recorder.publish(
                events::from_create("Distrib", &name, "PersistentVolumeClaim", &pvc.name_any(), Some(pvc.object_ref(&())))
            ).await.map_err(Error::KubeError)?;
        }


        let mut jobs = JobHandler::new(ctx.client.clone(), ns);
        let template = jobs.get_clone(name.as_str(), self.spec.login.clone());
        if jobs.have(clone_name.as_str()).await {
            info!("Patching {clone_name} Job");
            let _job = match jobs.apply_distrib(clone_name.as_str(), &template, "clone", name.as_str()).await {Ok(j)=>j,Err(_e)=>{
                let job = jobs.get(clone_name.as_str()).await.unwrap();
                recorder.publish(
                    events::from_delete("plan", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
                ).await.map_err(Error::KubeError)?;
                jobs.delete(clone_name.as_str()).await.unwrap();
                jobs.create_distrib(clone_name.as_str(), &template, "clone", name.as_str()).await.unwrap()
            }};
            // TODO: Detect if the job changed after the patch (or event better would change prior)
            // TODO: Send a patched event if changed
            /*debug!("Sending event for {clone_name} to finish Job");
            recorder.publish(
                events::from_patch("Distrib", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
            ).await.map_err(Error::KubeError)?;*/
            debug!("Waiting {clone_name} to finish Job");
            jobs.wait_max(clone_name.as_str(),2*60).await.map_err(Error::WaitError)?.map_err(Error::JobError)?;
            debug!("Waited {clone_name} OK");
        } else {
            info!("Creating {clone_name} Job");
            let job = jobs.create_distrib(clone_name.as_str(), &template, "clone", name.as_str()).await.unwrap();
            recorder.publish(
                events::from_create("Distrib", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
            ).await.map_err(Error::KubeError)?;
        }

        let cronjob_tmpl = serde_json::json!({
            "concurrencyPolicy": "Forbid",
            "schedule": self.spec.schedule,
            "jobTemplate": {
                "spec": {
                    "template": template
                }
            }
        });
        let mut crons = CronJobHandler::new(ctx.client.clone(), ns);
        if crons.have(clone_name.as_str()).await {
            info!("Patching {clone_name} CronJob");
            let _cjob = match crons.apply(clone_name.as_str(), &cronjob_tmpl, "clone", name.as_str()).await {Ok(j)=>j,Err(_e)=>{
                let job = crons.get(clone_name.as_str()).await.unwrap();
                recorder.publish(
                    events::from_delete("plan", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
                ).await.map_err(Error::KubeError)?;
                crons.delete(clone_name.as_str()).await.unwrap();
                crons.create(clone_name.as_str(), &template, "clone", name.as_str()).await.unwrap()
            }};
        } else {
            info!("Creating {clone_name} CronJob");
            let cron = crons.create(clone_name.as_str(), &cronjob_tmpl, "clone", name.as_str()).await.unwrap();
            recorder.publish(
                events::from_create("Distrib", &name, "CronJob", &cron.name_any(), Some(cron.object_ref(&())))
            ).await.map_err(Error::KubeError)?;
        }
        // If no events were received, check back every 15 minutes
        Ok(Action::requeue(Duration::from_secs(15 * 60)))
    }

    // Reconcile with finalize cleanup (the object was deleted)
    async fn cleanup(&self, ctx: Arc<Context>) -> Result<Action> {
        let client = ctx.client.clone();
        ctx.diagnostics.write().await.last_event = Utc::now();
        let reporter = ctx.diagnostics.read().await.reporter.clone();
        let recorder = Recorder::new(client.clone(), reporter, self.object_ref(&()));
        let name = self.name_any();
        let ns = ctx.client.default_namespace();
        let clone_name= format!("{name}-clone");

        let mut pvcs = PersistentVolumeClaimHandler::new(ctx.client.clone(), ns);
        if pvcs.have(format!("{name}-distrib").as_str()).await {
            info!("Deleting {name}-distrib PersistentVolumeClaim");
            let pvc = pvcs.get(format!("{name}-distrib").as_str()).await.unwrap();
            recorder.publish(
                events::from_delete("Distrib", &name, "PersistentVolumeClaim", &pvc.name_any(), Some(pvc.object_ref(&())))
            ).await.map_err(Error::KubeError)?;
            pvcs.delete(format!("{name}-distrib").as_str()).await.unwrap();
        }

        let mut jobs = JobHandler::new(ctx.client.clone(), ns);
        if jobs.have(clone_name.as_str()).await {
            info!("Deleting {clone_name} Job");
            let job = jobs.get(clone_name.as_str()).await.unwrap();
            recorder.publish(
                events::from_delete("Distrib", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
            ).await.map_err(Error::KubeError)?;
            jobs.delete(clone_name.as_str()).await.unwrap();
        }

        let mut crons = CronJobHandler::new(ctx.client.clone(), ns);
        if crons.have(clone_name.as_str()).await {
            info!("Deleting {clone_name} CronJob");
            let cron = crons.get(clone_name.as_str()).await.unwrap();
            recorder.publish(
                events::from_delete("Distrib", &name, "CronJob", &cron.name_any(), Some(cron.object_ref(&())))
            ).await.map_err(Error::KubeError)?;
            crons.delete(clone_name.as_str()).await.unwrap();
        }

        Ok(Action::await_change())
    }
}

#[must_use] pub fn error_policy(dist: Arc<Distrib>, error: &Error, ctx: Arc<Context>) -> Action {
    warn!("reconcile failed for {:?}: {:?}", dist.metadata.name, error);
    ctx.metrics.dist_reconcile_failure(&dist, error);
    Action::requeue(Duration::from_secs(5 * 60))
}
