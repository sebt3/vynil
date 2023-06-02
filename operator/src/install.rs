use crate::{OPERATOR, manager::Context, telemetry, Error, Result, Reconciler, jobs::JobHandler, events, cronjobs::CronJobHandler, secrets::SecretHandler};
use k8s_openapi::api::core::v1::Secret;
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
use base64::{Engine as _, engine::general_purpose};
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
        let mut crons = CronJobHandler::new(ctx.client.clone(), my_ns);
        let secret_name = format!("{ns}--{name}--secret");
        let plan_name = format!("{ns}--{name}--plan");
        let install_name = format!("{ns}--{name}--install");
        let dist_name = self.spec.distrib.as_str();
        let dist = match dists.get(dist_name).await {Ok(d) => d, Err(e) => {
            let mut errors: Vec<String> = Vec::new();
            errors.push(format!("{:?}", e));
            self.update_status_missing_distrib(client, OPERATOR, errors).await.map_err(Error::KubeError)?;
            return Err(Error::IllegalDistrib);
        }};
        // Validate that the requested package exist in that distrib
        if ! dist.have_component(self.spec.category.as_str(), self.spec.component.as_str()) {
            let mut errors: Vec<String> = Vec::new();
            errors.push(format!("{:} - {:} is not known from  {:?} distribution", self.spec.category.as_str(), self.spec.component.as_str(), dist_name));
            self.update_status_missing_component(client, OPERATOR, errors).await.map_err(Error::KubeError)?;
            if dist.status.is_some() {
                return Err(Error::IllegalInstall)
            } else { // the dist is not yet updated, wait for it for 60s
                return Ok(Action::requeue(Duration::from_secs(60)))
            }
        }
        let comp = dist.get_component(self.spec.category.as_str(), self.spec.component.as_str()).unwrap();
        if comp.use_authentik() && ! self.have_authentik() {
            self.update_status_missing_provider(client, OPERATOR, vec!["Authentik requiered configuration is missing".to_string()]).await.map_err(Error::KubeError)?;
            return Err(Error::IllegalInstall)
        } else if comp.use_authentik() {
            // Check that this authentik installation is there
            if let Some(providers) = self.spec.providers.clone() {
                if let Some(authentik) = providers.authentik {
                    let mut secrets = SecretHandler::new(ctx.client.clone(), authentik.namespace.as_str());
                    let secret: Secret = secrets.get(format!("{}-akadmin",authentik.name).as_str()).await.unwrap();
                    if let Some(data) = secret.data.clone() {
                        if ! data.contains_key("AUTHENTIK_BOOTSTRAP_TOKEN") {
                            self.update_status_missing_component(client, OPERATOR, vec!["Authentik requiered secret is missing values".to_string()]).await.map_err(Error::KubeError)?;
                            return Ok(Action::requeue(Duration::from_secs(60)))
                        }
                    } else {
                        self.update_status_missing_component(client, OPERATOR, vec!["Authentik requiered secret is missing".to_string()]).await.map_err(Error::KubeError)?;
                        return Ok(Action::requeue(Duration::from_secs(60)))
                    }
                } else {
                    self.update_status_missing_provider(client, OPERATOR, vec!["Authentik requiered configuration is missing".to_string()]).await.map_err(Error::KubeError)?;
                    return Err(Error::IllegalInstall)
                }
            } else {
                self.update_status_missing_provider(client, OPERATOR, vec!["Authentik requiered configuration is missing".to_string()]).await.map_err(Error::KubeError)?;
                return Err(Error::IllegalInstall)
            }
        }
        if comp.use_postgresql() && ! self.have_postgresql() {
            self.update_status_missing_provider(client, OPERATOR, vec!["PostgreSQL requiered configuration is missing".to_string()]).await.map_err(Error::KubeError)?;
            return Err(Error::IllegalInstall)
        } else if comp.use_postgresql() {
            // Check that the pgo postgresql password exist
            if let Some(providers) = self.spec.providers.clone() {
                if let Some(postgresql) = providers.postgresql {
                    let mut secrets = SecretHandler::new(ctx.client.clone(), postgresql.namespace.as_str());
                    let secret: Secret = secrets.get(format!("postgres.{}.credentials.postgresql.acid.zalan.do",postgresql.name).as_str()).await.unwrap();
                    if let Some(data) = secret.data.clone() {
                        if ! data.contains_key("username") || ! data.contains_key("password") {
                            self.update_status_missing_component(client, OPERATOR, vec!["PostgreSQL requiered secret is missing values".to_string()]).await.map_err(Error::KubeError)?;
                            return Ok(Action::requeue(Duration::from_secs(60)))
                        }
                    } else {
                        self.update_status_missing_component(client, OPERATOR, vec!["PostgreSQL requiered secret is missing".to_string()]).await.map_err(Error::KubeError)?;
                        return Ok(Action::requeue(Duration::from_secs(60)))
                    }
                } else {
                    self.update_status_missing_provider(client, OPERATOR, vec!["PostgreSQL requiered configuration is missing".to_string()]).await.map_err(Error::KubeError)?;
                    return Err(Error::IllegalInstall)
                }
            } else {
                self.update_status_missing_provider(client, OPERATOR, vec!["PostgreSQL requiered configuration is missing".to_string()]).await.map_err(Error::KubeError)?;
                return Err(Error::IllegalInstall)
            }
        }
        if comp.dependencies.is_some() {
            for dep in comp.dependencies.clone().unwrap() {
                // Validate that the dependencies are actually known to the package management
                if dep.dist.is_some() {
                    let dist = match dists.get(dep.dist.clone().unwrap().as_str()).await{Ok(d) => d, Err(e) => {
                        let mut errors: Vec<String> = Vec::new();
                        errors.push(format!("{:?}", e));
                        self.update_status_missing_distrib(client, OPERATOR, errors).await.map_err(Error::KubeError)?;
                        return Err(Error::IllegalDistrib);
                    }};
                    if !dist.have_component(dep.category.as_str(), dep.component.as_str()) {
                        let mut errors: Vec<String> = Vec::new();
                        errors.push(format!("{:} - {:} is not known from  {:?} distribution", dep.category.as_str(), dep.component.as_str(), dep.dist.clone().unwrap().as_str()));
                        self.update_status_missing_component(client, OPERATOR, errors).await.map_err(Error::KubeError)?;
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
                        self.update_status_missing_component(client, OPERATOR, errors).await.map_err(Error::KubeError)?;
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
                    self.update_status_missing_dependencies(client, OPERATOR, errors).await.map_err(Error::KubeError)?;
                    // TODO: evaluate if failing is not a better strategy here
                    return Ok(Action::requeue(Duration::from_secs(60)))
                } else {
                    let installs: Api<Install> = Api::namespaced(client.clone(), found_ns.as_str());
                    let install = installs.get(found_name.as_str()).await.unwrap();
                    if install.status.is_none() || install.status.unwrap().status.as_str() != STATUS_INSTALLED {
                        let mut errors: Vec<String> = Vec::new();
                        errors.push(format!("Install {:} - {:} is not yet ready", found_ns.as_str(), found_name.as_str()));
                        self.update_status_waiting_dependencies(client, OPERATOR, errors).await.map_err(Error::KubeError)?;
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
        if comp.use_postgresql() || comp.use_authentik() {
            // Prepare a secret
            let mut my_secrets = SecretHandler::new(ctx.client.clone(), my_ns);
            let mut my_secret = serde_json::json!({});
            if comp.use_authentik() {
                // Set AUTHENTIK_URL, AUTHENTIK_TOKEN secret values
                if let Some(providers) = self.spec.providers.clone() {
                    if let Some(authentik) = providers.authentik {
                        let mut secrets = SecretHandler::new(ctx.client.clone(), authentik.namespace.as_str());
                        let secret: Secret = secrets.get(format!("{}-akadmin",authentik.name).as_str()).await.unwrap();
                        if let Some(data) = secret.data {
                            if data.contains_key("AUTHENTIK_BOOTSTRAP_TOKEN") {
                                let token = data["AUTHENTIK_BOOTSTRAP_TOKEN"].clone();
                                my_secret["AUTHENTIK_TOKEN"] = serde_json::Value::String(general_purpose::STANDARD.encode(std::str::from_utf8(&token.0).unwrap()));
                                my_secret["AUTHENTIK_URL"] = serde_json::Value::String(general_purpose::STANDARD.encode(format!("{}.{}.svc",authentik.name, authentik.namespace)));
                            }
                        }
                    }
                }
            }
            if comp.use_postgresql() {
                // Set PGHOST, PGUSER, PGPASSWORD env variables from the actual pgo secret
                if let Some(providers) = self.spec.providers.clone() {
                    if let Some(postgresql) = providers.postgresql {
                        let mut secrets = SecretHandler::new(ctx.client.clone(), postgresql.namespace.as_str());
                        let secret: Secret = secrets.get(format!("postgres.{}.credentials.postgresql.acid.zalan.do",postgresql.name).as_str()).await.unwrap();
                        if let Some(data) = secret.data {
                            if data.contains_key("username") && data.contains_key("password") {
                                let username = data["username"].clone();
                                let password = data["password"].clone();
                                my_secret["PGHOST"] = serde_json::Value::String(general_purpose::STANDARD.encode(format!("{}.{}.svc",postgresql.name, postgresql.namespace)));
                                my_secret["PGUSER"] = serde_json::Value::String(general_purpose::STANDARD.encode(std::str::from_utf8(&username.0).unwrap()));
                                my_secret["PGPASSWORD"] = serde_json::Value::String(general_purpose::STANDARD.encode(std::str::from_utf8(&password.0).unwrap()));
                            }
                        }
                    }
                }
            }
            // Upsert the secret
            if !my_secrets.have(secret_name.as_str()).await {
                info!("Creating {secret_name} Secret");
                let scret = my_secrets.create(secret_name.as_str(), &my_secret).await.unwrap();
                debug!("Sending event {secret_name} Secret");
                recorder.publish(
                    events::from_create("Install", &name, "Secret", &scret.name_any(), Some(scret.object_ref(&())))
                ).await.map_err(Error::KubeError)?;
            } else {
                info!("Patching {secret_name} Secret");
                let _scret = match my_secrets.apply(secret_name.as_str(), &my_secret).await {Ok(j)=>j,Err(_e)=>{
                    let scret: k8s_openapi::api::core::v1::Secret = my_secrets.get(secret_name.as_str()).await.unwrap();
                    recorder.publish(
                        events::from_delete("plan", &name, "Secret", &scret.name_any(), Some(scret.object_ref(&())))
                    ).await.map_err(Error::KubeError)?;
                    my_secrets.delete(secret_name.as_str()).await.unwrap();
                    my_secrets.create(secret_name.as_str(), &my_secret).await.unwrap()
                }};
            }
        }

        let hashedself = crate::jobs::HashedSelf::new(ns.as_str(), name.as_str(), self.options_digest().as_str());
        let install_job = jobs.get_installs_install(&hashedself, self.spec.distrib.as_str(), self.spec.category.as_str(), self.spec.component.as_str(), self.spec.providers.clone());
        let plan_job = jobs.get_installs_plan(&hashedself, self.spec.distrib.as_str(), self.spec.category.as_str(), self.spec.component.as_str(), self.spec.providers.clone());

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
            self.update_status_planning(client, OPERATOR).await.map_err(Error::KubeError)?;
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
            self.update_status_installing(client, OPERATOR).await.map_err(Error::KubeError)?;
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
        let secret_name = format!("{ns}--{name}--secret");
        let mut my_secrets = SecretHandler::new(ctx.client.clone(), my_ns);

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
        if self.have_tfstate() {
            // Create the delete job
            let hashedself = crate::jobs::HashedSelf::new(ns.as_str(), name.as_str(), self.options_digest().as_str());
            let destroyer_job = jobs.get_installs_destroy(&hashedself, self.spec.distrib.as_str(), self.spec.category.as_str(), self.spec.component.as_str(), self.spec.providers.clone());

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
