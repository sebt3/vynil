use crate::{anonymize::scrub, dto::ScrubStats, error::DiagError};
use common::{
    Children, instanceservice::ServiceInstance, instancesystem::SystemInstance,
    instancetenant::TenantInstance,
};
use k8s_openapi::api::{batch::v1::Job as BatchJob, core::v1::Pod};
use kube::{Api, Client};

/// Get agent logs for an instance
pub async fn get_agent_log(
    client: &Client,
    namespace: &str,
    name: &str,
    vynil_namespace: &str,
) -> Result<(String, ScrubStats), DiagError> {
    // Find install Jobs owned by the instance
    let jobs = find_install_jobs(client, namespace, name).await?;

    let mut all_logs = String::new();

    for job in jobs {
        // Get pods for this job
        let pods = get_job_pods(client, namespace, &job.metadata.name.unwrap_or_default()).await?;

        for pod in pods {
            let pod_name = pod.metadata.name.clone().unwrap_or_default();
            let logs = get_pod_logs(client, namespace, &pod_name, None).await?;
            if !logs.is_empty() {
                all_logs.push_str(&format!("=== pod/{} ===\n", pod_name));
                all_logs.push_str(&logs);
                all_logs.push('\n');
            }
        }
    }

    if all_logs.is_empty() {
        return Ok((String::new(), ScrubStats::default()));
    }

    // Anonymize the logs
    let (scrubbed, stats) = scrub(&all_logs, client, namespace, vynil_namespace).await;

    Ok((scrubbed, stats))
}

/// Find install Jobs owned by an instance
async fn find_install_jobs(
    client: &Client,
    namespace: &str,
    instance_name: &str,
) -> Result<Vec<BatchJob>, DiagError> {
    let api: Api<BatchJob> = Api::namespaced(client.clone(), namespace);
    let job_list = api
        .list(&Default::default())
        .await
        .map_err(DiagError::KubeError)?;

    // Filter jobs owned by the instance
    Ok(job_list
        .items
        .into_iter()
        .filter(|job| {
            if let Some(owner_refs) = &job.metadata.owner_references {
                owner_refs
                    .iter()
                    .any(|owner| owner.controller == Some(true) && owner.name == instance_name)
            } else {
                false
            }
        })
        .collect())
}

/// Get pods for a job
async fn get_job_pods(client: &Client, namespace: &str, job_name: &str) -> Result<Vec<Pod>, DiagError> {
    let api: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let pod_list = api
        .list(&Default::default())
        .await
        .map_err(DiagError::KubeError)?;

    Ok(pod_list
        .items
        .into_iter()
        .filter(|pod| {
            if let Some(owner_refs) = &pod.metadata.owner_references {
                owner_refs
                    .iter()
                    .any(|owner| owner.kind == "Job" && owner.name == job_name)
            } else {
                false
            }
        })
        .collect())
}

/// Get logs from a pod
async fn get_pod_logs(
    client: &Client,
    namespace: &str,
    pod_name: &str,
    container: Option<&str>,
) -> Result<String, DiagError> {
    let api: Api<Pod> = Api::namespaced(client.clone(), namespace);

    // Current container logs (previous=false). `previous=true` would only return the *prior*
    // terminated instance — never the running container — which left every log endpoint empty.
    let params = kube::api::LogParams {
        container: container.map(|s| s.to_string()),
        previous: false,
        since_seconds: None,
        tail_lines: None,
        ..Default::default()
    };

    match api.logs(pod_name, &params).await {
        Ok(logs) => Ok(logs),
        Err(e) => {
            tracing::warn!("Failed to get logs for pod {}: {}", pod_name, e);
            Ok(String::new())
        }
    }
}

/// Get child logs for an instance
pub async fn get_child_logs(
    client: &Client,
    kind: &str,
    namespace: &str,
    instance_name: &str,
    log_since_hours: u64,
    log_cap_bytes: usize,
    vynil_namespace: &str,
) -> Result<(String, ScrubStats), DiagError> {
    // Get the instance to get its children
    let children = get_instance_children(client, kind, namespace, instance_name).await?;

    let mut all_logs = String::new();

    for child in children {
        if let Some(child_namespace) = child.namespace.as_deref().or(Some(namespace)) {
            // Get pods for this child (based on workload type)
            let pods = get_child_pods(client, child_namespace, &child).await?;

            for pod in pods {
                let pod_name = pod.metadata.name.clone().unwrap_or_default();
                let logs = get_pod_logs_with_options(
                    client,
                    child_namespace,
                    &pod_name,
                    log_since_hours,
                    log_cap_bytes,
                )
                .await?;

                if !logs.is_empty() {
                    all_logs.push_str(&format!(
                        "=== pod/{} ({}/{}) ===\n",
                        pod_name, child.kind, child.name
                    ));
                    all_logs.push_str(&logs);
                    all_logs.push('\n');
                }
            }
        }
    }

    if all_logs.is_empty() {
        return Ok((String::new(), ScrubStats::default()));
    }

    // Anonymize the logs
    let (scrubbed, stats) = scrub(&all_logs, client, namespace, vynil_namespace).await;

    Ok((scrubbed, stats))
}

/// Get all children of an instance, across every status category (befores/vitals/scalables/
/// others/posts for tenant & service ; systems for system). Generic JSON walk so a per-kind
/// schema difference (SystemInstance uses `systems`) does not silently drop children.
async fn get_instance_children(
    client: &Client,
    kind: &str,
    namespace: &str,
    name: &str,
) -> Result<Vec<Children>, DiagError> {
    let json = match kind {
        "tenantinstances" => {
            let api: Api<TenantInstance> = Api::namespaced(client.clone(), namespace);
            serde_json::to_value(api.get(name).await.map_err(DiagError::KubeError)?)
        }
        "serviceinstances" => {
            let api: Api<ServiceInstance> = Api::namespaced(client.clone(), namespace);
            serde_json::to_value(api.get(name).await.map_err(DiagError::KubeError)?)
        }
        "systeminstances" => {
            let api: Api<SystemInstance> = Api::namespaced(client.clone(), namespace);
            serde_json::to_value(api.get(name).await.map_err(DiagError::KubeError)?)
        }
        _ => return Err(DiagError::UnknownKind),
    }
    .map_err(DiagError::SerializationError)?;

    Ok(crate::collect::children::all_children_from_status(&json))
}

/// Resolve the pods of a child workload via its label selector (matchLabels), instead of
/// guessing from generic labels. Only pod-bearing workloads are handled; others yield nothing.
async fn get_child_pods(
    client: &Client,
    namespace: &str,
    child: &common::Children,
) -> Result<Vec<Pod>, DiagError> {
    use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, ReplicaSet, StatefulSet};

    let match_labels = match child.kind.as_str() {
        "Deployment" => {
            let api: Api<Deployment> = Api::namespaced(client.clone(), namespace);
            api.get(&child.name)
                .await
                .ok()
                .and_then(|w| w.spec)
                .map(|s| s.selector.match_labels)
        }
        "StatefulSet" => {
            let api: Api<StatefulSet> = Api::namespaced(client.clone(), namespace);
            api.get(&child.name)
                .await
                .ok()
                .and_then(|w| w.spec)
                .map(|s| s.selector.match_labels)
        }
        "DaemonSet" => {
            let api: Api<DaemonSet> = Api::namespaced(client.clone(), namespace);
            api.get(&child.name)
                .await
                .ok()
                .and_then(|w| w.spec)
                .map(|s| s.selector.match_labels)
        }
        "ReplicaSet" => {
            let api: Api<ReplicaSet> = Api::namespaced(client.clone(), namespace);
            api.get(&child.name)
                .await
                .ok()
                .and_then(|w| w.spec)
                .map(|s| s.selector.match_labels)
        }
        _ => None,
    };

    let Some(Some(labels)) = match_labels else {
        return Ok(Vec::new());
    };
    if labels.is_empty() {
        return Ok(Vec::new());
    }

    let selector = labels
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",");

    let api: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let lp = kube::api::ListParams::default().labels(&selector);
    Ok(api.list(&lp).await.map_err(DiagError::KubeError)?.items)
}

/// Get logs with since and cap options
async fn get_pod_logs_with_options(
    client: &Client,
    namespace: &str,
    pod_name: &str,
    log_since_hours: u64,
    log_cap_bytes: usize,
) -> Result<String, DiagError> {
    let api: Api<Pod> = Api::namespaced(client.clone(), namespace);

    let since_seconds = (log_since_hours * 3600) as i64;

    // Get logs for all containers in the pod
    let mut all_logs = String::new();

    // First get the pod to see what containers it has
    let pod = api.get(pod_name).await.map_err(DiagError::KubeError)?;

    if let Some(spec) = &pod.spec {
        for container in &spec.containers {
            // Current logs (previous=false) over the retained window.
            let params = kube::api::LogParams {
                container: Some(container.name.clone()),
                previous: false,
                since_seconds: Some(since_seconds),
                tail_lines: None,
                ..Default::default()
            };
            match api.logs(pod_name, &params).await {
                Ok(logs) if !logs.is_empty() => {
                    all_logs.push_str(&format!("=== container/{} ===\n", container.name));
                    all_logs.push_str(&cap(&logs, log_cap_bytes));
                    all_logs.push('\n');
                }
                Ok(_) => {}
                Err(e) => tracing::warn!("logs {}/{}: {}", pod_name, container.name, e),
            }

            // Additionally, previous terminated logs (crashloop) — best-effort, ignored if none.
            let prev = kube::api::LogParams {
                container: Some(container.name.clone()),
                previous: true,
                since_seconds: Some(since_seconds),
                tail_lines: None,
                ..Default::default()
            };
            if let Ok(logs) = api.logs(pod_name, &prev).await
                && !logs.is_empty()
            {
                all_logs.push_str(&format!("=== container/{} (previous) ===\n", container.name));
                all_logs.push_str(&cap(&logs, log_cap_bytes));
                all_logs.push('\n');
            }
        }
    }

    Ok(all_logs)
}

/// Cap a log string to `max` bytes (on a char boundary), marking truncation.
fn cap(logs: &str, max: usize) -> String {
    if logs.len() <= max {
        return logs.to_string();
    }
    let mut end = max;
    while end > 0 && !logs.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n... [truncated]", &logs[..end])
}

/// Get operator logs filtered for a specific instance
pub async fn get_operator_log(
    client: &Client,
    instance_namespace: &str,
    instance_name: &str,
    vynil_namespace: &str,
) -> Result<(String, ScrubStats), DiagError> {
    // Get pods in the vynil namespace with the vynil label
    let api: Api<Pod> = Api::namespaced(client.clone(), vynil_namespace);
    let pod_list = api
        .list(&Default::default())
        .await
        .map_err(DiagError::KubeError)?;

    let mut all_logs = String::new();
    let pods = pod_list.items;

    for pod in &pods {
        if let Some(labels) = &pod.metadata.labels
            && labels.get("app.kubernetes.io/name").is_some_and(|v| v == "vynil")
        {
            // This is a vynil operator pod
            let pod_name = pod.metadata.name.clone().unwrap_or_default();
            let logs = get_pod_logs(client, vynil_namespace, &pod_name, None).await?;

            // Filter logs for lines mentioning the instance
            let filtered_logs: String = logs
                .lines()
                .filter(|line| line.contains(instance_name) || line.contains(instance_namespace))
                .collect::<Vec<_>>()
                .join("\n");

            if !filtered_logs.is_empty() {
                all_logs.push_str(&format!("=== pod/{} ===\n", pod_name));
                all_logs.push_str(&filtered_logs);
                all_logs.push('\n');
            }
        }
    }

    // SECURITY: never fall back to dumping the whole controller log — that would leak every
    // other tenant's lines. If filtering matched nothing, return an explicit marker instead.
    if all_logs.is_empty() {
        return Ok((
            format!(
                "# no operator log lines matched instance {}/{} in the retained window\n",
                instance_namespace, instance_name
            ),
            ScrubStats::default(),
        ));
    }

    let (scrubbed, stats) = scrub(&all_logs, client, instance_namespace, vynil_namespace).await;
    Ok((scrubbed, stats))
}
