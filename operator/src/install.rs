use crate::{OPERATOR, manager::Context, telemetry, Error, Result, Reconciler, jobs::JobHandler, events, secrets::SecretHandler};
use package::script;
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
    let ns = inst.namespace(); // inst is namespace scoped
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
        let ns = self.namespace();
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
                        warn!("Will not trigger auto-install before the bootstrap job is completed, requeue");
                        recorder.publish(
                            events::from_check("Install", &name, "Bootstrap in progress, requeue".to_string(), None)
                        ).await.map_err(Error::KubeError)?;
                        return Ok(Action::requeue(Duration::from_secs(60)))
                    }
                }
            }
        }
        // Validate that the requested package exist in that distrib
        if ! dist.have_component(self.spec.category.as_str(), self.spec.component.as_str()) {
            self.update_status_missing_component(client, OPERATOR, vec!(format!("{:} - {:} is not known from  {:?} distribution", self.spec.category.as_str(), self.spec.component.as_str(), dist_name))).await.map_err(Error::KubeError)?;
            warn!("Missing component for {ns}.{name}");
            recorder.publish(
                events::from_check("Install", &name, "Missing component".to_string(), Some(format!("{:} - {:} is not known from  {:?} distribution", self.spec.category.as_str(), self.spec.component.as_str(), dist_name)))
            ).await.map_err(Error::KubeError)?;
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
                let note = missing[0].clone();
                warn!("Missing dependencies for {ns}.{name}: {note}");
                recorder.publish(
                    events::from_check("Install", &name, "Missing dependencies".to_string(), Some(note))
                ).await.map_err(Error::KubeError)?;
                self.update_status_missing_component(client, OPERATOR, missing).await.map_err(Error::KubeError)?;
                if should_fail {
                    return Err(Error::IllegalInstall)
                }
                return Ok(Action::requeue(Duration::from_secs(60)))
            }
        }
        // Use provided check script
        if comp.check.is_some() {
            let check = comp.check.clone().unwrap();
            let mut script  = script::Script::from_str(&check, script::new_base_context(
                self.spec.category.clone(),
                self.spec.component.clone(),
                name.clone(),
                &self.options()
            ));
            let stage = "check".to_string();
            let errors = match script.run_pre_stage(&stage) {
                Ok(_d) => Vec::new(),
                Err(e) => {
                    let mut missing: Vec<String> = Vec::new();
                    missing.push(format!("{e}"));
                    missing
                }
            };
            if ! errors.is_empty() {
                let note = errors[0].clone();
                warn!("Validation script failed for {ns}.{name}: {note}");
                recorder.publish(
                    events::from_check("Install", &name, "Validation failed".to_string(), Some(note))
                ).await.map_err(Error::KubeError)?;
                self.update_status_check_failed(client, OPERATOR, errors).await.map_err(Error::KubeError)?;
                return Ok(Action::requeue(Duration::from_secs(60)))
            } else {
                recorder.publish(
                    events::from_check("Install", &name, "Validation succeed".to_string(), None)
                ).await.map_err(Error::KubeError)?;
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
            recorder.publish(
                events::from_create("Install", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
            ).await.map_err(Error::KubeError)?;
            jobs.wait_max(agent_name.as_str(),2*60).await.map_err(Error::WaitError)?.map_err(Error::JobError)?;
        } else {
            let job = match jobs.apply_install(agent_name.as_str(), &agent_job, action, name.as_str(), ns.as_str()).await {Ok(j)=>j,Err(_e)=>{
                let job = jobs.get(agent_name.as_str()).await.unwrap();
                recorder.publish(
                    events::from_delete("Install", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
                ).await.map_err(Error::KubeError)?;
                jobs.delete(agent_name.as_str()).await.unwrap();
                info!("Recreating {agent_name} Job");
                recorder.publish(
                    events::from_create("Install", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
                ).await.map_err(Error::KubeError)?;
                jobs.create_install(agent_name.as_str(), &agent_job, action, name.as_str(), ns.as_str()).await.unwrap()
            }};
            if let Some(status) = job.status {
                if status.completion_time.is_none() {
                    info!("Waiting after {agent_name} Job");
                    recorder.publish(
                        events::from_check("Install", &name, "Bootstrap in progress, requeue".to_string(), None)
                    ).await.map_err(Error::KubeError)?;
                    jobs.wait_max(agent_name.as_str(),2*60).await.map_err(Error::WaitError)?.map_err(Error::JobError)?;
                }
            }
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
        let ns = self.namespace();
        let agent_name = format!("{ns}--{name}--agent");
        let deletor_name = format!("{ns}--{name}--delete");

        let secret_name = format!("{ns}--{name}--secret");
        let mut my_secrets = SecretHandler::new(ctx.client.clone(), my_ns);

        if self.have_tfstate() {
            // delete the agent job if any
            if jobs.have(agent_name.as_str()).await {
                // Force delete the install-job
                info!("Deleting {agent_name} Job");
                let job = jobs.get(agent_name.as_str()).await.unwrap();
                match recorder.publish(
                    events::from_delete("Install", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
                ).await { Ok(_) => {}, Err(e) => {debug!("While publishing event, we got {:?}", e)} };
                jobs.delete(agent_name.as_str()).await.unwrap();
            }
            // Create the delete job
            let hashedself = crate::jobs::HashedSelf::new(ns.as_str(), name.as_str(), self.options_digest().as_str(), self.spec.distrib.as_str(), "");
            let destroyer_job = jobs.get_installs_destroy(&hashedself, self.spec.category.as_str(), self.spec.component.as_str());

            info!("Creating {deletor_name} Job");
            let job = match jobs.apply_short_install(deletor_name.as_str(), &destroyer_job, "destroy", name.as_str(), ns.as_str()).await {Ok(j)=>j,Err(_e)=>{
                let job: k8s_openapi::api::batch::v1::Job = jobs.get(deletor_name.as_str()).await.unwrap();
                match recorder.publish(
                    events::from_delete("Install", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
                ).await { Ok(_) => {}, Err(e) => {debug!("While publishing event, we got {:?}", e)} };
                jobs.delete(deletor_name.as_str()).await.unwrap();
                jobs.create_short_install(deletor_name.as_str(), &destroyer_job, "destroy", name.as_str(), ns.as_str()).await.unwrap()
            }};
            match recorder.publish(
                events::from_create("Install", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
            ).await { Ok(_) => {}, Err(e) => {debug!("While publishing event, we got {:?}", e)} };
            // Wait up-to 5mn for it's completion
            match jobs.wait_max(deletor_name.as_str(), 5*60).await {
                Ok(_) => {},
                Err(_) => return Err(Error::TooLongDelete)
            }
            // Finally delete the destroyer job
            if jobs.have(deletor_name.as_str()).await {
                // Force delete the install-job
                info!("Deleting {deletor_name} Job");
                let job = jobs.get(deletor_name.as_str()).await.unwrap();
                match recorder.publish(
                    events::from_delete("Install", &name, "Job", &job.name_any(), Some(job.object_ref(&())))
                ).await { Ok(_) => {}, Err(e) => {debug!("While publishing event, we got {:?}", e)} };
                jobs.delete(deletor_name.as_str()).await.unwrap();
            }
        }

        if my_secrets.have(secret_name.as_str()).await {
            info!("Deleting {secret_name} Secret");
            let scret: k8s_openapi::api::core::v1::Secret = my_secrets.get(secret_name.as_str()).await.unwrap();
            match recorder.publish(
                events::from_delete("Install", &name, "Secret", &scret.name_any(), Some(scret.object_ref(&())))
            ).await { Ok(_) => {}, Err(e) => {debug!("While publishing event, we got {:?}", e)} };
            my_secrets.delete(secret_name.as_str()).await.unwrap();
        }

        Ok(Action::await_change())
    }
}

#[must_use] pub fn error_policy(inst: Arc<Install>, error: &Error, ctx: Arc<Context>) -> Action {
    warn!("reconcile failed for '{:?}.{:?}': {:?}", inst.metadata.namespace, inst.metadata.name, error);
    ctx.metrics.inst_reconcile_failure(&inst, error);
    Action::requeue(Duration::from_secs(5 * 60))
}
