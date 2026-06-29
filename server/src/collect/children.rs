use crate::{
    anonymize::scrub_json,
    dto::{ChildWithState, ScrubStats},
    error::DiagError,
};
use common::{
    Children, instanceservice::ServiceInstance, instancesystem::SystemInstance,
    instancetenant::TenantInstance,
};
use kube::{Api, Client};
use serde_json;

/// Get children information for an instance
pub async fn get_children(
    client: &Client,
    kind: &str,
    namespace: &str,
    name: &str,
    vynil_namespace: &str,
) -> Result<(Vec<ChildWithState>, ScrubStats), DiagError> {
    // Get the instance to extract children
    let children = match kind {
        "tenantinstances" => get_tenant_children(client, namespace, name).await?,
        "serviceinstances" => get_service_children(client, namespace, name).await?,
        "systeminstances" => get_system_children(client, namespace, name).await?,
        _ => return Err(DiagError::UnknownKind),
    };

    // Get current state for each child
    let mut result = Vec::new();
    let mut total_stats = ScrubStats::default();

    for child in children {
        let (state, stats) = get_child_state(client, &child, namespace, vynil_namespace).await?;
        total_stats.distinct += stats.distinct;
        total_stats.occurrences += stats.occurrences;

        result.push(ChildWithState { child, state });
    }

    Ok((result, total_stats))
}

/// Extract children from a TenantInstance
async fn get_tenant_children(
    client: &Client,
    namespace: &str,
    name: &str,
) -> Result<Vec<Children>, DiagError> {
    let api: Api<TenantInstance> = Api::namespaced(client.clone(), namespace);
    let instance = api.get(name).await.map_err(DiagError::KubeError)?;

    extract_children_from_instance(&instance)
}

/// Extract children from a ServiceInstance
async fn get_service_children(
    client: &Client,
    namespace: &str,
    name: &str,
) -> Result<Vec<Children>, DiagError> {
    let api: Api<ServiceInstance> = Api::namespaced(client.clone(), namespace);
    let instance = api.get(name).await.map_err(DiagError::KubeError)?;

    extract_children_from_instance(&instance)
}

/// Extract children from a SystemInstance
async fn get_system_children(
    client: &Client,
    namespace: &str,
    name: &str,
) -> Result<Vec<Children>, DiagError> {
    let api: Api<SystemInstance> = Api::namespaced(client.clone(), namespace);
    let instance = api.get(name).await.map_err(DiagError::KubeError)?;

    extract_children_from_instance(&instance)
}

/// Status categories that hold `Children` lists across all instance kinds.
/// `systems` is SystemInstance-specific — omitting it dropped all SystemInstance children.
const CHILD_CATEGORIES: &[&str] = &[
    "befores",
    "vitals",
    "scalables",
    "others",
    "posts",
    "systems",
    "services",
];

/// Extract every child from an instance's `status`, across all categories, from its JSON form.
pub fn all_children_from_status(instance_json: &serde_json::Value) -> Vec<Children> {
    let Some(status) = instance_json.get("status").and_then(|s| s.as_object()) else {
        return Vec::new();
    };
    let mut children = Vec::new();
    for cat in CHILD_CATEGORIES {
        if let Some(arr) = status.get(*cat).and_then(|v| v.as_array()) {
            children.extend(extract_children_from_json(arr));
        }
    }
    children
}

/// Extract children from an instance using serde_json to handle the different status shapes.
fn extract_children_from_instance(
    instance: &(impl serde::Serialize + std::fmt::Debug),
) -> Result<Vec<Children>, DiagError> {
    match serde_json::to_value(instance) {
        Ok(json) => Ok(all_children_from_status(&json)),
        Err(_) => Ok(Vec::new()),
    }
}

/// Extract Children from a JSON array (entries that don't shape as Children are skipped).
fn extract_children_from_json(array: &[serde_json::Value]) -> Vec<Children> {
    array
        .iter()
        .filter_map(|value| serde_json::from_value::<Children>(value.clone()).ok())
        .collect()
}

/// Get current state for a child
async fn get_child_state(
    client: &Client,
    child: &Children,
    instance_namespace: &str,
    vynil_namespace: &str,
) -> Result<(Option<serde_json::Value>, ScrubStats), DiagError> {
    let namespace = child.namespace.as_deref().unwrap_or(instance_namespace);

    // Try to get the actual Kubernetes object
    // This is a simplified approach - in a real implementation, you'd need to handle
    // the different API groups and kinds dynamically
    match child.kind.as_str() {
        "Deployment" => {
            use k8s_openapi::api::apps::v1::Deployment;
            let api: Api<Deployment> = Api::namespaced(client.clone(), namespace);
            match api.get(&child.name).await {
                Ok(deployment) => {
                    let (scrubbed, stats) = scrub_json(
                        serde_json::to_value(deployment).unwrap(),
                        client,
                        namespace,
                        vynil_namespace,
                    )
                    .await?;
                    Ok((Some(scrubbed), stats))
                }
                Err(_) => Ok((None, ScrubStats::default())),
            }
        }
        "StatefulSet" => {
            use k8s_openapi::api::apps::v1::StatefulSet;
            let api: Api<StatefulSet> = Api::namespaced(client.clone(), namespace);
            match api.get(&child.name).await {
                Ok(statefulset) => {
                    let (scrubbed, stats) = scrub_json(
                        serde_json::to_value(statefulset).unwrap(),
                        client,
                        namespace,
                        vynil_namespace,
                    )
                    .await?;
                    Ok((Some(scrubbed), stats))
                }
                Err(_) => Ok((None, ScrubStats::default())),
            }
        }
        "DaemonSet" => {
            use k8s_openapi::api::apps::v1::DaemonSet;
            let api: Api<DaemonSet> = Api::namespaced(client.clone(), namespace);
            match api.get(&child.name).await {
                Ok(daemonset) => {
                    let (scrubbed, stats) = scrub_json(
                        serde_json::to_value(daemonset).unwrap(),
                        client,
                        namespace,
                        vynil_namespace,
                    )
                    .await?;
                    Ok((Some(scrubbed), stats))
                }
                Err(_) => Ok((None, ScrubStats::default())),
            }
        }
        "Service" => {
            use k8s_openapi::api::core::v1::Service;
            let api: Api<Service> = Api::namespaced(client.clone(), namespace);
            match api.get(&child.name).await {
                Ok(service) => {
                    let (scrubbed, stats) = scrub_json(
                        serde_json::to_value(service).unwrap(),
                        client,
                        namespace,
                        vynil_namespace,
                    )
                    .await?;
                    Ok((Some(scrubbed), stats))
                }
                Err(_) => Ok((None, ScrubStats::default())),
            }
        }
        "ConfigMap" => {
            use k8s_openapi::api::core::v1::ConfigMap;
            let api: Api<ConfigMap> = Api::namespaced(client.clone(), namespace);
            match api.get(&child.name).await {
                Ok(configmap) => {
                    let (scrubbed, stats) = scrub_json(
                        serde_json::to_value(configmap).unwrap(),
                        client,
                        namespace,
                        vynil_namespace,
                    )
                    .await?;
                    Ok((Some(scrubbed), stats))
                }
                Err(_) => Ok((None, ScrubStats::default())),
            }
        }
        _ => {
            // For unknown kinds, return None with no stats
            Ok((None, ScrubStats::default()))
        }
    }
}
