use k8s_openapi::api::batch::v1::Job;
use kube::{
    api::{Api, DeleteParams, PostParams, ListParams, PatchParams, Patch},
    runtime::wait::{await_condition, conditions},
    Client,
};
use either::Either;
use crate::{AGENT_IMAGE, OPERATOR};
use k8s::install::ProviderConfigs;

pub struct JobHandler {
    api: Api<Job>,
}

fn install_container(ns: &str, name: &str, hash: &str) -> serde_json::Value {
    serde_json::json!({
        "args":[],
        "image": std::env::var("AGENT_IMAGE").unwrap_or_else(|_| AGENT_IMAGE.to_string()),
        "imagePullPolicy": "IfNotPresent",
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

fn clone_container(name: &str) -> serde_json::Value {
    serde_json::json!({
        "args":["clone"],
        "image": std::env::var("AGENT_IMAGE").unwrap_or_else(|_| AGENT_IMAGE.to_string()),
        "imagePullPolicy": "IfNotPresent",
        "name": "clone",
        "env": [{
            "name": "DIST_NAME",
            "value": name
        },{
            "name": "LOG_LEVEL",
            "value": "debug"
        },{
            "name": "RUST_LOG",
            "value": "info,controller=debug,agent=debug"
        }],
        "volumeMounts": [{
            "name": "dist",
            "mountPath": "/work",
            "subPath": name
        }],
})
}

fn get_action(ns: &str, name: &str, hash: &str, act: &str, cfg: Option<ProviderConfigs>) -> serde_json::Value {
    let mut action = install_container(ns, name, hash);
    action["name"] = serde_json::Value::String(act.to_string());
    action["args"] = serde_json::Value::Array([action["name"].clone()].into());
    if let Some(ref cfg) = cfg {
        if cfg.authentik.is_some() || cfg.postgresql.is_some() {
            action["envFrom"] = serde_json::Value::Array([serde_json::json!({
                "secretRef": {
                    "name": format!("{ns}--{name}--secret")
                }
            })].into());
        }
    }
    action
}

fn get_templater(ns: &str, name: &str, hash: &str, distrib: &str, category: &str, component: &str) -> serde_json::Value {
    let mut templater = install_container(ns, name, hash);
    templater["name"] = serde_json::Value::String("template".to_string());
    templater["args"] = serde_json::Value::Array([
        templater["name"].clone(),
        serde_json::Value::String("-s".to_string()),
        serde_json::Value::String(format!("/src/{}/{}/", category, component))
    ].into());
    templater["volumeMounts"] = serde_json::Value::Array([serde_json::json!({
        "name": "dist",
        "mountPath": "/src",
        "subPath": distrib
    }),serde_json::json!({
        "name": "package",
        "mountPath": "/dest"
    })].into());
    templater
}


impl JobHandler {
    #[must_use] pub fn new(cl: Client, ns: &str) -> JobHandler {
        JobHandler {
            api: Api::namespaced(cl, ns),
        }
    }

    pub fn get_clone(&self, name: &str) -> serde_json::Value {
        serde_json::json!({
            "spec": {
                "serviceAccount": "vynil-agent",
                "serviceAccountName": "vynil-agent",
                "restartPolicy": "Never",
                "containers": [clone_container(name)],
                "volumes": [{
                    "name": "dist",
                    "persistentVolumeClaim": {
                        "claimName": format!("{name}-distrib")
                    }
                }],
                "securityContext": {
                    "fsGroup": 65534,
                    "runAsUser": 65534,
                    "runAsGroup": 65534
                }
            }
        })
    }

    pub fn get_installs_plan(&self, ns: &str, name: &str, hash: &str, distrib: &str, category: &str, component: &str, cfg: Option<ProviderConfigs>) -> serde_json::Value {
        serde_json::json!({
            "spec": {
                "serviceAccount": "vynil-agent",
                "serviceAccountName": "vynil-agent",
                "restartPolicy": "Never",
                "initContainers": [get_templater(ns, name, hash, distrib, category, component)],
                "containers": [get_action(ns, name, hash, "plan", cfg)],
                "volumes": [{
                    "name": "dist",
                    "persistentVolumeClaim": {
                        "claimName": format!("{}-distrib", distrib)
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
        })
    }

    pub fn get_installs_destroy(&self, ns: &str, name: &str, hash: &str, distrib: &str, category: &str, component: &str, cfg: Option<ProviderConfigs>) -> serde_json::Value {
        serde_json::json!({
            "spec": {
                "serviceAccount": "vynil-agent",
                "serviceAccountName": "vynil-agent",
                "restartPolicy": "Never",
                "initContainers": [get_templater(ns, name, hash, distrib, category, component)],
                "containers": [get_action(ns, name, hash, "destroy", cfg)],
                "volumes": [{
                    "name": "dist",
                    "persistentVolumeClaim": {
                        "claimName": format!("{}-distrib", distrib)
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
        })
    }

    pub fn get_installs_install(&self, ns: &str, name: &str, hash: &str, distrib: &str, category: &str, component: &str, cfg: Option<ProviderConfigs>) -> serde_json::Value {
        serde_json::json!({
            "spec": {
                "serviceAccount": "vynil-agent",
                "serviceAccountName": "vynil-agent",
                "restartPolicy": "Never",
                "initContainers": [get_templater(ns, name, hash, distrib, category, component),get_action(ns, name, hash, "plan", cfg.clone())],
                "containers": [get_action(ns, name, hash, "install", cfg)],
                "volumes": [{
                    "name": "dist",
                    "persistentVolumeClaim": {
                        "claimName": format!("{}-distrib", distrib)
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
        })
    }

    pub async fn have(&mut self, name: &str) -> bool {
        let lp = ListParams::default();
        let list = self.api.list(&lp).await.unwrap();
        for job in list {
            if job.metadata.name.clone().unwrap() == name {
                return true;
            }
        }
        false
    }

    pub async fn get(&mut self, name: &str) -> Result<Job, kube::Error> {
        self.api.get(name).await
    }

    pub async fn create(&mut self, name: &str, template: &serde_json::Value) -> Result<Job, kube::Error> {
        let data = serde_json::from_value(serde_json::json!({
            "apiVersion": "batch/v1",
            "kind": "Job",
            "metadata": {
                "name": name,
            },
            "spec": {
                "backoffLimit": 3,
                "parallelism": 1,
                "template": template
            }
        })).unwrap();
        self.api.create(&PostParams::default(), &data).await
    }

    pub async fn apply(&mut self, name: &str, template: &serde_json::Value) -> Result<Job, kube::Error> {
        if self.have(name).await {
            let params = PatchParams::apply(OPERATOR);
            let patch = Patch::Apply(serde_json::json!({
                "apiVersion": "batch/v1",
                "kind": "Job",
                "metadata": {
                    "name": name,
                },
                "spec": {
                    "template": template
                }
            }));
            self.api.patch(name, &params, &patch).await
        } else {
            self.create(name, template).await
        }
    }

    pub async fn wait(&mut self, name: &str) -> Result<Result<Option<Job>, kube::runtime::wait::Error>, tokio::time::error::Elapsed> {
        self.wait_max(name, 20).await
    }
    pub async fn wait_max(&mut self, name: &str, secs: u64) -> Result<Result<Option<Job>, kube::runtime::wait::Error>, tokio::time::error::Elapsed> {
        let cond = await_condition(self.api.clone(), name, conditions::is_job_completed());
        tokio::time::timeout(std::time::Duration::from_secs(secs), cond).await
    }
    pub async fn delete(&mut self, name: &str) -> Result<Either<Job, kube::core::response::Status>, kube::Error> {
        self.api.delete(name, &DeleteParams::background()).await
    }
}
