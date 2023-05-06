use crate::{AGENT_IMAGE, manager::Context, telemetry, Error, Result, Reconciler, jobs::JobHandler, events, cronjobs::CronJobHandler};

use chrono::Utc;
use kube::{
    api::{Api, ResourceExt},
    runtime::{
        controller::Action,
        events::Recorder,
        finalizer::{finalizer, Event as Finalizer},
    },
    Resource,
};
use std::sync::Arc;
use tokio::time::Duration;
use tracing::{Span, debug, field, info, instrument, warn};
use async_trait::async_trait;
pub use k8s::install::{Install,InstallStatus};
static INSTALL_FINALIZER: &str = "installs.vynil.solidite.fr";


#[instrument(skip(ctx, inst), fields(trace_id))]
pub async fn reconcile(inst: Arc<Install>, ctx: Arc<Context>) -> Result<Action> {
    let trace_id = telemetry::get_trace_id();
    Span::current().record("trace_id", &field::display(&trace_id));
    let _mes = ctx.metrics.inst_count_and_measure();
    let ns = inst.namespace().unwrap(); // inst is namespace scoped
    let insts: Api<Install> = Api::namespaced(ctx.client.clone(), &ns);

    info!("Reconciling Install \"{}\" in {}", inst.name_any(), ns);
    finalizer(&insts, INSTALL_FINALIZER, inst, |event| async {
        match event {
            Finalizer::Apply(inst) => inst.reconcile(ctx.clone()).await,
            Finalizer::Cleanup(inst) => inst.cleanup(ctx.clone()).await,
        }
    }).await.map_err(|e| Error::FinalizerError(Box::new(e)))
}

fn container(ns: &str, name: &str, hash: &str) -> serde_json::Value {
    serde_json::json!({
        "args":[],
        "image": std::env::var("AGENT_IMAGE").unwrap_or_else(|_| AGENT_IMAGE.to_string()),
        "imagePullPolicy": "Always",
        "env": [{
            "name": "NAMESPACE",
            "value": ns
        },{
            "name": "NAME",
            "value": name
        },{
            "name": "hash",
            "value": hash
        },{
            "name": "LOG_LEVEL",
            "value": "debug"
        },{
            "name": "RUST_LOG",
            "value": "info,controller=debug,agent=debug"
        }],
        "volumeMounts": [{
            "name": "package",
            "mountPath": "/src"
        }],
    })
}

#[async_trait]
impl Reconciler for Install {
    // Reconcile (for non-finalizer related changes)
    async fn reconcile(&self, ctx: Arc<Context>) -> Result<Action> {
        let client = ctx.client.clone();
        ctx.diagnostics.write().await.last_event = Utc::now();
        let reporter = ctx.diagnostics.read().await.reporter.clone();
        let recorder = Recorder::new(client.clone(), reporter, self.object_ref(&()));
        let name = self.name_any();
        let ns = self.namespace().unwrap();
        let my_ns = ctx.client.default_namespace();
        let mut jobs = JobHandler::new(ctx.client.clone(), my_ns);
        let mut crons = CronJobHandler::new(ctx.client.clone(), my_ns);
        let plan_name = format!("{ns}--{name}--plan");
        let install_name = format!("{ns}--{name}--install");
        // TODO: Validate that the requested package exist in that distrib Set illegal if not
        /*if name == "illegal" {
            return Err(Error::IllegalInstall); // error names show up in metrics
        }*/

        if self.should_plan() && !self.options_status() && jobs.have(plan_name.as_str()).await {
            // Force delete the plan-job
            info!("Deleting {plan_name} Job");
            let job = jobs.get(plan_name.as_str()).await.unwrap();
            recorder.publish(
                events::from_delete("Install", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
            ).await.map_err(Error::KubeError)?;
            jobs.delete(plan_name.as_str()).await.unwrap();
        } else if !self.should_plan() && !self.options_status() && jobs.have(install_name.as_str()).await {
            // Force delete the install-job
            info!("Deleting {install_name} Job");
            let job = jobs.get(install_name.as_str()).await.unwrap();
            recorder.publish(
                events::from_delete("Install", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
            ).await.map_err(Error::KubeError)?;
            jobs.delete(install_name.as_str()).await.unwrap();
        }

        let mut templater = container(ns.as_str(),name.as_str(), self.options_digest().as_str());
        templater["name"] = serde_json::Value::String("template".to_string());
        templater["args"] = serde_json::Value::Array([
            templater["name"].clone(),
            serde_json::Value::String("-s".to_string()),
            serde_json::Value::String(format!("/src/{}/{}/",self.spec.category, self.spec.component))
        ].into());
        templater["volumeMounts"] = serde_json::Value::Array([serde_json::json!({
            "name": "dist",
            "mountPath": "/src",
            "subPath": self.spec.distrib
        }),serde_json::json!({
            "name": "package",
            "mountPath": "/dest"
        })].into());
        let mut planner = container(ns.as_str(),name.as_str(), self.options_digest().as_str());
        planner["name"] = serde_json::Value::String("plan".to_string());
        planner["args"] = serde_json::Value::Array([planner["name"].clone()].into());
        let mut installer = container(ns.as_str(),name.as_str(), self.options_digest().as_str());
        installer["name"] = serde_json::Value::String("install".to_string());
        installer["args"] = serde_json::Value::Array([installer["name"].clone()].into());
        let install_job = serde_json::json!({
            "spec": {
                "serviceAccount": "vynil-agent",
                "serviceAccountName": "vynil-agent",
                "restartPolicy": "Never",
                "initContainers": [templater, planner],
                "containers": [installer],
                "volumes": [{
                    "name": "dist",
                    "persistentVolumeClaim": {
                        "claimName": format!("{}-distrib", self.spec.distrib)
                    }
                },{
                    "name": "package",
                    "emptyDir": {
                        "sizeLimit": "100Mi"
                    }
                }],
                "securityContext": {
                    "fsGroup": 65534,
                    "runAsUser": 65534,
                    "runAsGroup": 65534
                }
            }
        });
        let plan_job = serde_json::json!({
            "spec": {
                "serviceAccount": "vynil-agent",
                "serviceAccountName": "vynil-agent",
                "restartPolicy": "Never",
                "initContainers": [templater],
                "containers": [planner],
                "volumes": [{
                    "name": "dist",
                    "persistentVolumeClaim": {
                        "claimName": format!("{}-distrib", self.spec.distrib)
                    }
                },{
                    "name": "package",
                    "emptyDir": {
                        "sizeLimit": "100Mi"
                    }
                }],
                "securityContext": {
                    "fsGroup": 65534,
                    "runAsUser": 65534,
                    "runAsGroup": 65534
                }
            }
        });

        if self.spec.schedule.is_some() {
            let cronjob_install = serde_json::json!({
                "concurrencyPolicy": "Forbid",
                "schedule": self.spec.schedule.clone().unwrap(),
                "jobTemplate": {
                    "spec": {
                        "template": install_job
                    }
                }
            });
            let cronjob_plan = serde_json::json!({
                "concurrencyPolicy": "Forbid",
                "schedule": self.spec.schedule.clone().unwrap(),
                "jobTemplate": {
                    "spec": {
                        "template": plan_job
                    }
                }
            });

            if self.should_plan() && !crons.have(plan_name.as_str()).await {
                info!("Creating {plan_name} CronJob");
                let cron = crons.create(plan_name.as_str(), &cronjob_plan).await.unwrap();
                debug!("Sending event {plan_name} CronJob");
                recorder.publish(
                    events::from_create("Install", &name, "CronJob", &cron.name_any(), Some(cron.object_ref(&())))
                ).await.map_err(Error::KubeError)?;
            } else if self.should_plan() {
                info!("Patching {plan_name} CronJob");
                let _cron = match crons.apply(plan_name.as_str(), &cronjob_plan).await {Ok(j)=>j,Err(_e)=>{
                    let cron: k8s_openapi::api::batch::v1::CronJob = crons.get(plan_name.as_str()).await.unwrap();
                    recorder.publish(
                        events::from_delete("plan", &name, "CronJob", &cron.name_any(), Some(cron.object_ref(&())))
                    ).await.map_err(Error::KubeError)?;
                    crons.delete(plan_name.as_str()).await.unwrap();
                    crons.create(plan_name.as_str(), &cronjob_plan).await.unwrap()
                }};
            } else if !crons.have(install_name.as_str()).await {
                info!("Creating {install_name} CronJob");
                let cron = crons.create(install_name.as_str(), &cronjob_install).await.unwrap();
                debug!("Sending event for {install_name} CronJob");
                recorder.publish(
                    events::from_create("Install", &name, "CronJob", &cron.name_any(), Some(cron.object_ref(&())))
                ).await.map_err(Error::KubeError)?;
            } else {
                info!("Patching {install_name} CronJob");
                let _cron = match crons.apply(install_name.as_str(), &cronjob_install).await {Ok(j)=>j,Err(_e)=>{
                    let cron = crons.get(install_name.as_str()).await.unwrap();
                    recorder.publish(
                        events::from_delete("Install", &name, "CronJob", &cron.name_any(), Some(cron.object_ref(&())))
                    ).await.map_err(Error::KubeError)?;
                    crons.delete(install_name.as_str()).await.unwrap();
                    crons.create(install_name.as_str(), &cronjob_install).await.unwrap()
                }};
            }
        }

        if self.should_plan() && !jobs.have(plan_name.as_str()).await {
            info!("Creating {plan_name} Job");
            let job = jobs.create(plan_name.as_str(), &plan_job).await.unwrap();
            debug!("Sending event {plan_name} Job");
            recorder.publish(
                events::from_create("Install", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
            ).await.map_err(Error::KubeError)?;
            debug!("Waiting {plan_name} to finish Job");
            jobs.wait_max(plan_name.as_str(),2*60).await.map_err(Error::WaitError)?.map_err(Error::JobError)?;
            debug!("Waited {plan_name} OK");
        } else if self.should_plan() {
            info!("Patching {plan_name} Job");
            let _job = match jobs.apply(plan_name.as_str(), &plan_job).await {Ok(j)=>j,Err(_e)=>{
                let job = jobs.get(plan_name.as_str()).await.unwrap();
                recorder.publish(
                    events::from_delete("plan", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
                ).await.map_err(Error::KubeError)?;
                jobs.delete(plan_name.as_str()).await.unwrap();
                jobs.create(plan_name.as_str(), &plan_job).await.unwrap()
            }};
            // TODO: Detect if the job changed after the patch (or event better would change prior)
            // TODO: Send a patched event if changed
            /*debug!("Sending event for {plan_name} to finish Job");
            recorder.publish(
                events::from_patch("Install", &name, "Job", &_job.name_any(), Some(_job.object_ref(&())))
            ).await.map_err(Error::KubeError)?;*/
            debug!("Waiting {plan_name} to finish Job");
            jobs.wait_max(plan_name.as_str(),2*60).await.map_err(Error::WaitError)?.map_err(Error::JobError)?;
            debug!("Waited {plan_name} OK");
        } else if !jobs.have(install_name.as_str()).await {
            info!("Creating {install_name} Job");
            let job = jobs.create(install_name.as_str(), &install_job).await.unwrap();
            debug!("Sending event for {install_name} Job");
            recorder.publish(
                events::from_create("Install", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
            ).await.map_err(Error::KubeError)?;
            debug!("Waiting {install_name} to finish Job");
            jobs.wait_max(install_name.as_str(),8*60).await.map_err(Error::WaitError)?.map_err(Error::JobError)?;
            debug!("Waited {install_name} OK");
        } else {
            info!("Patching {install_name} Job");
            let _job = match jobs.apply(install_name.as_str(), &install_job).await {Ok(j)=>j,Err(_e)=>{
                let job = jobs.get(install_name.as_str()).await.unwrap();
                recorder.publish(
                    events::from_delete("Install", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
                ).await.map_err(Error::KubeError)?;
                jobs.delete(install_name.as_str()).await.unwrap();
                jobs.create(install_name.as_str(), &install_job).await.unwrap()
            }};
            // TODO: Detect if the job changed after the patch (or event better would change prior)
            // TODO: Send a patched event if changed
            /*debug!("Sending event for {install_name} Job");
            recorder.publish(
                events::from_patch("Install", &name, "Job", &_job.name_any(), Some(_job.object_ref(&())))
            ).await.map_err(Error::KubeError)?;*/
            debug!("Waiting {install_name} to finish Job");
            jobs.wait_max(install_name.as_str(),8*60).await.map_err(Error::WaitError)?.map_err(Error::JobError)?;
            debug!("Waited {install_name} OK");
        }
        Ok(Action::requeue(Duration::from_secs(5 * 60)))
    }

    // Reconcile with finalize cleanup (the object was deleted)
    async fn cleanup(&self, ctx: Arc<Context>) -> Result<Action> {
        let client = ctx.client.clone();
        ctx.diagnostics.write().await.last_event = Utc::now();
        let reporter = ctx.diagnostics.read().await.reporter.clone();
        let recorder = Recorder::new(client.clone(), reporter, self.object_ref(&()));
        let my_ns = ctx.client.default_namespace();
        let mut jobs = JobHandler::new(ctx.client.clone(), my_ns);
        let name = self.name_any();
        let ns = self.namespace().unwrap();
        let plan_name = format!("{ns}--{name}--plan");
        let install_name = format!("{ns}--{name}--install");
        let destroyer_name = format!("{ns}--{name}--destroy");

        if jobs.have(plan_name.as_str()).await {
            // Force delete the plan-job
            info!("Deleting {plan_name} Job");
            let job = jobs.get(plan_name.as_str()).await.unwrap();
            recorder.publish(
                events::from_delete("Install", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
            ).await.map_err(Error::KubeError)?;
            jobs.delete(plan_name.as_str()).await.unwrap();
        }
        if jobs.have(install_name.as_str()).await {
            // Force delete the install-job
            info!("Deleting {install_name} Job");
            let job = jobs.get(install_name.as_str()).await.unwrap();
            recorder.publish(
                events::from_delete("Install", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
            ).await.map_err(Error::KubeError)?;
            jobs.delete(install_name.as_str()).await.unwrap();
        }
        // Create the delete job
        let mut templater = container(ns.as_str(),name.as_str(), self.options_digest().as_str());
        templater["name"] = serde_json::Value::String("template".to_string());
        templater["args"] = serde_json::Value::Array([
            templater["name"].clone(),
            serde_json::Value::String("-s".to_string()),
            serde_json::Value::String(format!("/src/{}/{}/",self.spec.category, self.spec.component))
        ].into());
        templater["volumeMounts"] = serde_json::Value::Array([serde_json::json!({
            "name": "dist",
            "mountPath": "/src",
            "subPath": self.spec.distrib
        }),serde_json::json!({
            "name": "package",
            "mountPath": "/dest"
        })].into());
        let mut destroyer = container(ns.as_str(),name.as_str(), self.options_digest().as_str());
        destroyer["name"] = serde_json::Value::String("destroy".to_string());
        destroyer["args"] = serde_json::Value::Array([destroyer["name"].clone()].into());
        let destroyer_job = serde_json::json!({
            "spec": {
                "serviceAccount": "vynil-agent",
                "serviceAccountName": "vynil-agent",
                "restartPolicy": "Never",
                "initContainers": [templater],
                "containers": [destroyer],
                "volumes": [{
                    "name": "dist",
                    "persistentVolumeClaim": {
                        "claimName": format!("{}-distrib", self.spec.distrib)
                    }
                },{
                    "name": "package",
                    "emptyDir": {
                        "sizeLimit": "100Mi"
                    }
                }],
                "securityContext": {
                    "fsGroup": 65534,
                    "runAsUser": 65534,
                    "runAsGroup": 65534
                }
            }
        });
        info!("Creating {destroyer_name} Job");
        let job = match jobs.apply(destroyer_name.as_str(), &destroyer_job).await {Ok(j)=>j,Err(_e)=>{
            let job = jobs.get(destroyer_name.as_str()).await.unwrap();
            recorder.publish(
                events::from_delete("Install", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
            ).await.map_err(Error::KubeError)?;
            jobs.delete(destroyer_name.as_str()).await.unwrap();
            jobs.create(destroyer_name.as_str(), &destroyer_job).await.unwrap()
        }};
        recorder.publish(
            events::from_create("Install", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
        ).await.map_err(Error::KubeError)?;
        // Wait up-to 5mn for it's completion
        match jobs.wait_max(destroyer_name.as_str(), 5*60).await {
            Ok(_) => {},
            Err(_) => return Err(Error::TooLongDelete)
        }
        // Finally delete the destroyer job
        if jobs.have(destroyer_name.as_str()).await {
            // Force delete the install-job
            info!("Deleting {destroyer_name} Job");
            let job = jobs.get(destroyer_name.as_str()).await.unwrap();
            recorder.publish(
                events::from_delete("Install", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
            ).await.map_err(Error::KubeError)?;
            jobs.delete(destroyer_name.as_str()).await.unwrap();
        }

        Ok(Action::await_change())
    }
}

#[must_use] pub fn error_policy(inst: Arc<Install>, error: &Error, ctx: Arc<Context>) -> Action {
    warn!("reconcile failed: {:?}", error);
    ctx.metrics.inst_reconcile_failure(&inst, error);
    Action::requeue(Duration::from_secs(5 * 60))
}
