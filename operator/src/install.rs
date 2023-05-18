use crate::{AGENT_IMAGE, OPERATOR, manager::Context, telemetry, Error, Result, Reconciler, jobs::JobHandler, events, cronjobs::CronJobHandler};
use k8s_openapi::api::core::v1::Namespace;
use chrono::Utc;
use kube::{
    api::{Api, ListParams, ResourceExt},
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
pub use k8s::install::{Install,InstallStatus, STATUS_INSTALLED};
pub use k8s::distrib::{Distrib, ComponentDependency};
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
        let dists: Api<Distrib> = Api::all(client.clone());
        let name = self.name_any();
        let ns = self.namespace().unwrap();
        let my_ns = ctx.client.default_namespace();
        let mut jobs = JobHandler::new(ctx.client.clone(), my_ns);
        let mut crons = CronJobHandler::new(ctx.client.clone(), my_ns);
        let plan_name = format!("{ns}--{name}--plan");
        let install_name = format!("{ns}--{name}--install");
        let dist_name = self.spec.distrib.as_str();
        let dist = match dists.get(dist_name).await {Ok(d) => d, Err(e) => {
            let mut errors: Vec<String> = Vec::new();
            errors.push(format!("{:?}", e));
            self.update_status_missing_distrib(client, OPERATOR, errors).await;
            return Err(Error::IllegalDistrib);
        }};
        // Validate that the requested package exist in that distrib
        if ! dist.have_component(self.spec.category.as_str(), self.spec.component.as_str()) {
            let mut errors: Vec<String> = Vec::new();
            errors.push(format!("{:} - {:} is not known from  {:?} distribution", self.spec.category.as_str(), self.spec.component.as_str(), dist_name));
            self.update_status_missing_component(client, OPERATOR, errors).await;
            if dist.status.is_some() {
                return Err(Error::IllegalInstall)
            } else { // the dist is not yet updated, wait for it for 60s
                return Ok(Action::requeue(Duration::from_secs(60)))
            }
        }
        let comp = dist.get_component(self.spec.category.as_str(), self.spec.component.as_str()).unwrap();
        if comp.dependencies.is_some() {
            for dep in comp.dependencies.clone().unwrap() {
                // Validate that the dependencies are actually known to the package management
                if dep.dist.is_some() {
                    let dist = match dists.get(dep.dist.clone().unwrap().as_str()).await{Ok(d) => d, Err(e) => {
                        let mut errors: Vec<String> = Vec::new();
                        errors.push(format!("{:?}", e));
                        self.update_status_missing_distrib(client, OPERATOR, errors).await;
                        return Err(Error::IllegalDistrib);
                    }};
                    if !dist.have_component(dep.category.as_str(), dep.component.as_str()) {
                        let mut errors: Vec<String> = Vec::new();
                        errors.push(format!("{:} - {:} is not known from  {:?} distribution", dep.category.as_str(), dep.component.as_str(), dep.dist.clone().unwrap().as_str()));
                        self.update_status_missing_component(client, OPERATOR, errors).await;
                        if dist.status.is_some() {
                            return Err(Error::IllegalInstall)
                        } else { // the dist is not yet updated, wait for it for 60s
                            return Ok(Action::requeue(Duration::from_secs(60)))
                        }
                    }
                } else {
                    let mut found = false;
                    for dist in dists.list(&ListParams::default()).await.unwrap() {
                        if dist.have_component(dep.category.as_str(), dep.component.as_str()) {
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        let mut errors: Vec<String> = Vec::new();
                        errors.push(format!("{:} - {:} is not known from any distribution", dep.category.as_str(), dep.component.as_str()));
                        self.update_status_missing_component(client, OPERATOR, errors).await;
                        return Ok(Action::requeue(Duration::from_secs(60)))
                    }
                }
                // Validate that the dependencies are actually installed
                //TODO: support for only current namespace
                let mut found = false;
                let mut found_ns = String::new();
                let mut found_name = String::new();
                let namespaces: Api<Namespace> = Api::all(client.clone());
                for ns in namespaces.list(&ListParams::default()).await.unwrap() {
                    let installs: Api<Install> = Api::namespaced(client.clone(), ns.metadata.name.clone().unwrap().as_str());
                    for install in installs.list(&ListParams::default()).await.unwrap() {
                        if install.spec.component.as_str() == dep.component.as_str() && install.spec.category.as_str() == dep.category.as_str() {
                            found = true;
                            found_ns = ns.metadata.name.clone().unwrap();
                            found_name = install.metadata.name.clone().unwrap();
                            break;
                        }
                    }
                    if found {
                        break;
                    }
                }
                if ! found {
                    // TODO: should collect all issues before returning
                    let mut errors: Vec<String> = Vec::new();
                    errors.push(format!("{:} - {:} is not installed in any namespace", dep.category.as_str(), dep.component.as_str()));
                    self.update_status_missing_dependencies(client, OPERATOR, errors).await;
                    // TODO: evaluate if failing is not a better strategy here
                    return Ok(Action::requeue(Duration::from_secs(60)))
                } else {
                    let installs: Api<Install> = Api::namespaced(client.clone(), found_ns.as_str());
                    let install = installs.get(found_name.as_str()).await.unwrap();
                    if install.status.is_none() || install.status.unwrap().status.as_str() != STATUS_INSTALLED {
                        let mut errors: Vec<String> = Vec::new();
                        errors.push(format!("Install {:} - {:} is not yet ready", found_ns.as_str(), found_name.as_str()));
                        self.update_status_waiting_dependencies(client, OPERATOR, errors).await;
                        return Ok(Action::requeue(Duration::from_secs(60)))
                    }
                }
            }
        }

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
            self.update_status_planning(client, OPERATOR).await;
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
            self.update_status_installing(client, OPERATOR).await;
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
