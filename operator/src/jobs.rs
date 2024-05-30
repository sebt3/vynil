use k8s_openapi::api::batch::v1::Job;
use kube::{
    api::{Api, DeleteParams, PostParams, ListParams, PatchParams, Patch},
    runtime::wait::{await_condition, conditions},
    Client,
};
use either::Either;
use crate::{AGENT_IMAGE, OPERATOR};
use k8s::distrib::DistribAuthent;

pub struct HashedSelf {
    ns: String,
    name: String,
    hash: String,
    distrib: String,
    commit_id: String,
    conditions: String
}

impl HashedSelf {
    #[must_use] pub fn new(ns: &str, name: &str, hash: &str, distrib: &str, commit_id: &str, conditions: &str) -> HashedSelf {
        HashedSelf {
            ns: ns.to_string(),
            name: name.to_string(),
            hash: hash.to_string(),
            distrib: distrib.to_string(),
            commit_id: commit_id.to_string(),
            conditions: conditions.to_string()
        }
    }
}

pub struct JobHandler {
    api: Api<Job>,
}

fn install_container(hself: &HashedSelf) -> serde_json::Value {
    let level = std::env::var("AGENT_LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
    serde_json::json!({
        "args":[],
        "image": std::env::var("AGENT_IMAGE").unwrap_or_else(|_| AGENT_IMAGE.to_string()),
        "imagePullPolicy": "IfNotPresent",
        "env": [{
            "name": "NAMESPACE",
            "value": hself.ns
        },{
            "name": "NAME",
            "value": hself.name
        },{
            "name": "OPTIONS_HASH",
            "value": hself.hash
        },{
            "name": "COMMIT_ID",
            "value": hself.commit_id
        },{
            "name": "CONDITIONS",
            "value": hself.conditions
        },{
            "name": "LOG_LEVEL",
            "value": level
        },{
            "name": "RUST_BACKTRACE",
            "value": "1"
        },{
            "name": "RUST_LOG",
            "value": format!("{},controller={},agent={}", level, level, level)
        }],
        "volumeMounts": [{
            "name": "dist",
            "mountPath": "/dist",
            "subPath": hself.distrib,
            "readOnly": true
        },{
            "name": "package",
            "mountPath": "/src"
        }],
    })
}

fn clone_container(name: &str, auth: Option<DistribAuthent>) -> serde_json::Value {
    let level = std::env::var("AGENT_LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
    let mut mounts = vec!(serde_json::json!({
        "name": "dist",
        "mountPath": "/work",
        "subPath": name
    }));
    if let Some(auth) = auth {
        if auth.ssh_key.is_some() {
            mounts.push(serde_json::json!({
                "name": "ssh",
                "mountPath": "/var/lib/vynil/keys",
            }));
        }
        if auth.git_credentials.is_some() {
            mounts.push(serde_json::json!({
                "name": "creds",
                "mountPath": "/var/lib/vynil/git-credentials",
                "subPath": "git-credentials",
            }));
        }
    }
    serde_json::json!({
        "args":["clone"],
        "image": std::env::var("AGENT_IMAGE").unwrap_or_else(|_| AGENT_IMAGE.to_string()),
        "imagePullPolicy": "IfNotPresent",
        "name": "clone",
        "env": [{
            "name": "DIST_NAME",
            "value": name
        },{
            "name": "RUST_BACKTRACE",
            "value": "1"
        },{
            "name": "LOG_LEVEL",
            "value": level
        },{
            "name": "RUST_LOG",
            "value": format!("{},controller={},agent={}", level, level, level)
        },{
            "name": "GIT_SSH_COMMAND",
            "value": "ssh -o UserKnownHostsFile=/dev/null -o StrictHostKeyChecking=no -i /var/lib/vynil/keys/private"
        }],
        "volumeMounts": mounts,
    })
}

fn get_action(hself: &HashedSelf, act: &str) -> serde_json::Value {
    let mut action = install_container(hself);
    action["name"] = serde_json::Value::String(act.to_string());
    action["args"] = serde_json::Value::Array([action["name"].clone()].into());
    action
}

fn get_templater(hself: &HashedSelf, category: &str, component: &str, target: &str) -> serde_json::Value {
    let mut templater = install_container(hself);
    templater["name"] = serde_json::Value::String("template".to_string());
    templater["args"] = serde_json::Value::Array([
        templater["name"].clone(),
        serde_json::Value::String("-s".to_string()),
        serde_json::Value::String(format!("/src/{}/{}/", category, component)),
        serde_json::Value::String("-t".to_string()),
        serde_json::Value::String(target.to_string())
    ].into());
    templater["volumeMounts"] = serde_json::Value::Array([serde_json::json!({
        "name": "dist",
        "mountPath": "/src",
        "subPath": hself.distrib,
        "readOnly": true
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

    pub fn get_clone(&self, name: &str, auth: Option<DistribAuthent>) -> serde_json::Value {
        let mut volumes = vec!(serde_json::json!({
            "name": "dist",
            "persistentVolumeClaim": {
                "claimName": format!("{name}-distrib")
            }
        }));
        if let Some(auth) = auth.clone() {
            if let Some(ref ssh) = auth.ssh_key {
                volumes.push(serde_json::json!({
                    "name": "ssh",
                    "secret": {
                        "secretName": ssh.name.as_str(),
                        "defaultMode": 0o400,
                        "items": [{
                            "key": ssh.key.as_str(),
                            "path": "private"
                        }]
                    }
                }));
            }
            if let Some(ref cred) = auth.git_credentials {
                volumes.push(serde_json::json!({
                    "name": "creds",
                    "secret": {
                        "secretName": cred.name.as_str(),
                        "defaultMode": 0o400,
                        "items": [{
                            "key": cred.key.as_str(),
                            "path": "git-credentials"
                        }]
                    }
                }));
            }
        }
        serde_json::json!({
            "spec": {
                "serviceAccount": "vynil-agent",
                "serviceAccountName": "vynil-agent",
                "restartPolicy": "Never",
                "containers": [clone_container(name, auth)],
                "volumes": volumes,
                "securityContext": {
                    "fsGroup": 65534,
                    "runAsUser": 65534,
                    "runAsGroup": 65534
                }
            }
        })
    }

    fn get_installs_spec(&self, hashedself: &HashedSelf, init_containers: &Vec<serde_json::Value>, containers: &Vec<serde_json::Value>) -> serde_json::Value {
        serde_json::json!({
            "metadata": {
                "annotations": {
                    "mayfly.cloud.namecheap.com/expire": "120h"
                },
            },
            "spec": {
                "serviceAccount": "vynil-agent",
                "serviceAccountName": "vynil-agent",
                "restartPolicy": "Never",
                "initContainers": init_containers,
                "containers": containers,
                "volumes": [{
                    "name": "dist",
                    "persistentVolumeClaim": {
                        "claimName": format!("{}-distrib", hashedself.distrib)
                    }
                },{
                    "name": "package",
                    "emptyDir": {
                        "sizeLimit": "500Mi"
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

    pub fn get_installs_plan(&self, hashedself: &HashedSelf, category: &str, component: &str) -> serde_json::Value {
        self.get_installs_spec(hashedself,
            &vec!(get_templater(hashedself, category, component, "plan")),
            &vec!(get_action(hashedself, "plan"))
        )
    }

    pub fn get_installs_destroy(&self, hashedself: &HashedSelf, category: &str, component: &str) -> serde_json::Value {
        self.get_installs_spec(hashedself,
            &vec!(get_templater(hashedself, category, component, "destroy")),
            &vec!(get_action(hashedself, "destroy"))
        )
    }

    pub fn get_installs_install(&self, hashedself: &HashedSelf, category: &str, component: &str) -> serde_json::Value {
        self.get_installs_spec(hashedself,
            &vec![get_templater(hashedself, category, component, "install"),get_action(hashedself, "plan")],
            &vec!(get_action(hashedself, "install"))
        )
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

    pub async fn create_distrib(&mut self, name: &str, template: &serde_json::Value, action: &str, distrib_name: &str) -> Result<Job, kube::Error> {
        let data = serde_json::from_value(serde_json::json!({
            "apiVersion": "batch/v1",
            "kind": "Job",
            "metadata": {
                "name": name,
                "labels": {
                    "app": "vynil",
                    "component": "agent",
                    "action": action,
                    "distrib.name": distrib_name,
                },
            },
            "spec": {
                "backoffLimit": 3,
                "parallelism": 1,
                "template": template
            }
        })).unwrap();
        self.api.create(&PostParams::default(), &data).await
    }
    pub async fn create_install(&mut self, name: &str, template: &serde_json::Value, action: &str, install_name: &str, install_namespace: &str) -> Result<Job, kube::Error> {
        let data = serde_json::from_value(serde_json::json!({
            "apiVersion": "batch/v1",
            "kind": "Job",
            "metadata": {
                "name": name,
                "labels": {
                    "app": "vynil",
                    "component": "agent",
                    "action": action,
                    "install.name": install_name,
                    "install.namespace": install_namespace,
                },
            },
            "spec": {
                "backoffLimit": 3,
                "parallelism": 1,
                "template": template
            }
        })).unwrap();
        self.api.create(&PostParams::default(), &data).await
    }

    pub async fn create_short_install(&mut self, name: &str, template: &serde_json::Value, action: &str, install_name: &str, install_namespace: &str) -> Result<Job, kube::Error> {
        let data = serde_json::from_value(serde_json::json!({
            "apiVersion": "batch/v1",
            "kind": "Job",
            "metadata": {
                "name": name,
                "annotations": {
                    "mayfly.cloud.namecheap.com/expire": "1h"
                },
                "labels": {
                    "app": "vynil",
                    "component": "agent",
                    "action": action,
                    "install.name": install_name,
                    "install.namespace": install_namespace,
                },
            },
            "spec": {
                "backoffLimit": 3,
                "parallelism": 1,
                "template": template
            }
        })).unwrap();
        self.api.create(&PostParams::default(), &data).await
    }

    pub async fn apply_install(&mut self, name: &str, template: &serde_json::Value, action: &str, install_name: &str, install_namespace: &str) -> Result<Job, kube::Error> {
        if self.have(name).await {
            let params = PatchParams::apply(OPERATOR);
            let patch = Patch::Apply(serde_json::json!({
                "apiVersion": "batch/v1",
                "kind": "Job",
                "metadata": {
                    "name": name,
                    "labels": {
                        "app": "vynil",
                        "component": "agent",
                        "action": action,
                        "install.name": install_name,
                        "install.namespace": install_namespace,
                    },
                },
                "spec": {
                    "template": template
                }
            }));
            self.api.patch(name, &params, &patch).await
        } else {
            self.create_install(name, template, action, install_name, install_namespace).await
        }
    }
    pub async fn apply_distrib(&mut self, name: &str, template: &serde_json::Value, action: &str, distrib_name: &str) -> Result<Job, kube::Error> {
        if self.have(name).await {
            let params = PatchParams::apply(OPERATOR);
            let patch = Patch::Apply(serde_json::json!({
                "apiVersion": "batch/v1",
                "kind": "Job",
                "metadata": {
                    "name": name,
                    "labels": {
                        "app": "vynil",
                        "component": "agent",
                        "action": action,
                        "distrib.name": distrib_name,
                    },
                },
                "spec": {
                    "template": template
                }
            }));
            self.api.patch(name, &params, &patch).await
        } else {
            self.create_distrib(name, template, action, distrib_name).await
        }
    }

    pub async fn apply_short_install(&mut self, name: &str, template: &serde_json::Value, action: &str, install_name: &str, install_namespace: &str) -> Result<Job, kube::Error> {
        if self.have(name).await {
            let params = PatchParams::apply(OPERATOR);
            let patch = Patch::Apply(serde_json::json!({
                "apiVersion": "batch/v1",
                "kind": "Job",
                "metadata": {
                    "name": name,
                    "labels": {
                        "app": "vynil",
                        "component": "agent",
                        "action": action,
                        "install.name": install_name,
                        "install.namespace": install_namespace,
                    },
                    "annotations": {
                        "mayfly.cloud.namecheap.com/expire": "1h"
                    },
                },
                "spec": {
                    "template": template
                }
            }));
            self.api.patch(name, &params, &patch).await
        } else {
            self.create_short_install(name, template, action, install_name, install_namespace).await
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
