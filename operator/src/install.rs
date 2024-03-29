use crate::{OPERATOR, manager::Context, telemetry, Error, Result, Reconciler, jobs::JobHandler, events, secrets::SecretHandler};
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
//use base64::{Engine as _, engine::general_purpose};
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
        let agent_name = format!("{ns}--{name}--agent");
        let dist_name = self.spec.distrib.as_str();
        let dist = match dists.get(dist_name).await {Ok(d) => d, Err(e) => {
            self.update_status_missing_distrib(client, OPERATOR, vec!(format!("{:?}", e))).await.map_err(Error::KubeError)?;
            return Err(Error::IllegalDistrib);
        }};
        //TODO: label the install with the distrib, component and category so searching installs is simple
        if ns == my_ns && self.spec.distrib == "core" && self.spec.component == "vynil" && self.spec.category == "core" {
            // Auto-installing here, should wait for the bootstrap process to be done
            if jobs.have("vynil-bootstrap").await {
                let bootstrap = jobs.get("vynil-bootstrap").await.unwrap();
                if let Some(status) = bootstrap.status {
                    if status.completion_time.is_none() {
                        return Ok(Action::requeue(Duration::from_secs(60)))
                    }
                }
            }
        }
        // Validate that the requested package exist in that distrib
        if ! dist.have_component(self.spec.category.as_str(), self.spec.component.as_str()) {
            self.update_status_missing_component(client, OPERATOR, vec!(format!("{:} - {:} is not known from  {:?} distribution", self.spec.category.as_str(), self.spec.component.as_str(), dist_name))).await.map_err(Error::KubeError)?;
            if dist.status.is_some() {
                return Err(Error::IllegalInstall)
            } else { // the dist is not yet updated, wait for it for 60s
                return Ok(Action::requeue(Duration::from_secs(60)))
            }
        }
        let comp = dist.get_component(self.spec.category.as_str(), self.spec.component.as_str()).unwrap();
        if self.status.is_some() && self.status.clone().unwrap().status.as_str() == STATUS_INSTALLED && self.status.clone().unwrap().digest.as_str() == comp.commit_id.as_str() {
            // Nothing to do since component is already installed at current commit_id
            return Ok(Action::requeue(Duration::from_secs(5 * 60)))
        }
        if comp.dependencies.is_some() {
            let mut missing: Vec<String> = Vec::new();
            let mut should_fail = false;
            for dep in comp.dependencies.clone().unwrap() {
                // Validate that the dependencies are actually known to the package management
                if dep.dist.is_some() {
                    let dist = match dists.get(dep.dist.clone().unwrap().as_str()).await{Ok(d) => d, Err(e) => {
                        self.update_status_missing_distrib(client, OPERATOR, vec!(format!("{:?}", e))).await.map_err(Error::KubeError)?;
                        return Err(Error::IllegalDistrib);
                    }};
                    if !dist.have_component(dep.category.as_str(), dep.component.as_str()) {
                        missing.push(format!("{:} - {:} is not known from  {:?} distribution", dep.category.as_str(), dep.component.as_str(), dep.dist.clone().unwrap().as_str()));
                        if dist.status.is_some() {
                            should_fail = true;
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
                        missing.push(format!("{:} - {:} is not known from any distribution", dep.category.as_str(), dep.component.as_str()));
                    }
                }
                // Validate that the dependencies are actually installed
                //TODO: support for only current namespace
                let mut found = false;
                let mut found_ns = String::new();
                let mut found_name = String::new();
                let namespaces: Api<Namespace> = Api::all(client.clone());
                // TODO: sort the namespace with name close to the current namespace first so we found the right one
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
                    missing.push(format!("{:} - {:} is not installed in any namespace", dep.category.as_str(), dep.component.as_str()));
                } else {
                    let installs: Api<Install> = Api::namespaced(client.clone(), found_ns.as_str());
                    let install = installs.get(found_name.as_str()).await.unwrap();

                    if install.status.is_none() || install.status.unwrap().commit_id.is_empty() {
                        missing.push(format!("Install {:} - {:} is not yet ready", found_ns.as_str(), found_name.as_str()));
                    }
                }
            }
            if ! missing.is_empty() {
                self.update_status_missing_component(client, OPERATOR, missing).await.map_err(Error::KubeError)?;
                if should_fail {
                    return Err(Error::IllegalInstall)
                }
                return Ok(Action::requeue(Duration::from_secs(60)))
            }
        }

        let hashedself = crate::jobs::HashedSelf::new(ns.as_str(), name.as_str(), self.options_digest().as_str(), self.spec.distrib.as_str(), &comp.commit_id);
        let agent_job = if self.should_plan() {
            jobs.get_installs_plan(&hashedself, self.spec.category.as_str(), self.spec.component.as_str())
        } else {
            jobs.get_installs_install(&hashedself, self.spec.category.as_str(), self.spec.component.as_str())
        };
        let action = if self.should_plan() {
            "plan"
        } else {
            "install"
        };

        if !jobs.have(agent_name.as_str()).await {
            info!("Creating {agent_name} Job");
            self.update_status_agent_started(client, OPERATOR).await.map_err(Error::KubeError)?;
            let job = jobs.create_install(agent_name.as_str(), &agent_job, action, name.as_str(), ns.as_str()).await.unwrap();
            debug!("Sending event {agent_name} Job");
            recorder.publish(
                events::from_create("Install", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
            ).await.map_err(Error::KubeError)?;
            debug!("Waiting {agent_name} to finish Job");
            jobs.wait_max(agent_name.as_str(),2*60).await.map_err(Error::WaitError)?.map_err(Error::JobError)?;
            debug!("Waited {agent_name} OK");
        } else {
            info!("Patching {agent_name} Job");
            let _job = match jobs.apply_install(agent_name.as_str(), &agent_job, action, name.as_str(), ns.as_str()).await {Ok(j)=>j,Err(_e)=>{
                let job = jobs.get(agent_name.as_str()).await.unwrap();
                recorder.publish(
                    events::from_delete("plan", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
                ).await.map_err(Error::KubeError)?;
                jobs.delete(agent_name.as_str()).await.unwrap();
                jobs.create_install(agent_name.as_str(), &agent_job, action, name.as_str(), ns.as_str()).await.unwrap()
            }};
            // TODO: Detect if the job changed after the patch (or event better would change prior)
            // TODO: Send a patched event if changed
            /*debug!("Sending event for {plan_name} to finish Job");
            recorder.publish(
                events::from_patch("Install", &name, "Job", &_job.name_any(), Some(_job.object_ref(&())))
            ).await.map_err(Error::KubeError)?;*/
            debug!("Waiting {agent_name} to finish Job");
            jobs.wait_max(agent_name.as_str(),2*60).await.map_err(Error::WaitError)?.map_err(Error::JobError)?;
            debug!("Waited {agent_name} OK");
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
        let agent_name = format!("{ns}--{name}--agent");

        let secret_name = format!("{ns}--{name}--secret");
        let mut my_secrets = SecretHandler::new(ctx.client.clone(), my_ns);

        if self.have_tfstate() {
            // Create the delete job
            let hashedself = crate::jobs::HashedSelf::new(ns.as_str(), name.as_str(), self.options_digest().as_str(), self.spec.distrib.as_str(), "");
            let destroyer_job = jobs.get_installs_destroy(&hashedself, self.spec.category.as_str(), self.spec.component.as_str());

            info!("Creating {agent_name} Job");
            let job = match jobs.apply_short_install(agent_name.as_str(), &destroyer_job, "destroy", name.as_str(), ns.as_str()).await {Ok(j)=>j,Err(_e)=>{
                let job = jobs.get(agent_name.as_str()).await.unwrap();
                recorder.publish(
                    events::from_delete("Install", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
                ).await.map_err(Error::KubeError)?;
                jobs.delete(agent_name.as_str()).await.unwrap();
                jobs.create_short_install(agent_name.as_str(), &destroyer_job, "destroy", name.as_str(), ns.as_str()).await.unwrap()
            }};
            recorder.publish(
                events::from_create("Install", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
            ).await.map_err(Error::KubeError)?;
            // Wait up-to 5mn for it's completion
            match jobs.wait_max(agent_name.as_str(), 5*60).await {
                Ok(_) => {},
                Err(_) => return Err(Error::TooLongDelete)
            }
            // Finally delete the destroyer job
            if jobs.have(agent_name.as_str()).await {
                // Force delete the install-job
                info!("Deleting {agent_name} Job");
                let job = jobs.get(agent_name.as_str()).await.unwrap();
                recorder.publish(
                    events::from_delete("Install", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
                ).await.map_err(Error::KubeError)?;
                jobs.delete(agent_name.as_str()).await.unwrap();
            }
        }

        if my_secrets.have(secret_name.as_str()).await {
            info!("Deleting {secret_name} Secret");
            let scret: k8s_openapi::api::core::v1::Secret = my_secrets.get(secret_name.as_str()).await.unwrap();
            recorder.publish(
                events::from_delete("Install", &name, "Secret", &scret.name_any(), Some(scret.object_ref(&())))
            ).await.map_err(Error::KubeError)?;
            my_secrets.delete(secret_name.as_str()).await.unwrap();
        }

        Ok(Action::await_change())
    }
}

#[must_use] pub fn error_policy(inst: Arc<Install>, error: &Error, ctx: Arc<Context>) -> Action {
    warn!("reconcile failed: {:?}", error);
    ctx.metrics.inst_reconcile_failure(&inst, error);
    Action::requeue(Duration::from_secs(5 * 60))
}
