use crate::{
    dto::{ClusterInfo, NodeInfo, StorageClassInfo},
    error::DiagError,
};
use k8s_openapi::api::{core::v1::Node, networking::v1::IngressClass, storage::v1::StorageClass};
use kube::{Api, Client};

/// Get cluster information
pub async fn get_cluster_info(client: &Client, vynil_namespace: &str) -> Result<ClusterInfo, DiagError> {
    // Get nodes
    let nodes = get_nodes(client).await?;

    // Get distribution
    let distribution = detect_distribution(client).await?;

    // Get Kubernetes version
    let kubernetes_version = get_apiserver_version(client).await?;

    // Get vynil version
    let vynil_version = get_vynil_version(client, vynil_namespace).await?;

    // Get storage classes
    let storage_classes = get_storage_classes(client).await?;

    // Get ingress classes
    let ingress_classes = get_ingress_classes(client).await?;

    Ok(ClusterInfo {
        nodes,
        distribution,
        kubernetes_version,
        vynil_version,
        storage_classes,
        ingress_classes,
    })
}

/// Get all nodes in the cluster
async fn get_nodes(client: &Client) -> Result<Vec<NodeInfo>, DiagError> {
    let api: Api<Node> = Api::all(client.clone());
    let node_list = api
        .list(&Default::default())
        .await
        .map_err(DiagError::KubeError)?;

    let mut nodes = Vec::new();
    for node in node_list.items {
        let name = node.metadata.name.clone().unwrap_or_default();

        // Extract roles from labels
        let roles = extract_roles(&node);

        // Extract instance type, arch, OS from labels
        let labels = node.metadata.labels.as_ref();
        let instance_type = labels
            .and_then(|labels| labels.get("node.kubernetes.io/instance-type").cloned())
            .or_else(|| labels.and_then(|labels| labels.get("k8s.amazonaws.com/instance-type").cloned()))
            .or_else(|| labels.and_then(|labels| labels.get("cloud.google.com/machine-type").cloned()));

        let arch = labels
            .and_then(|labels| labels.get("kubernetes.io/arch").cloned())
            .or_else(|| labels.and_then(|labels| labels.get("beta.kubernetes.io/arch").cloned()));

        let os = labels
            .and_then(|labels| labels.get("kubernetes.io/os").cloned())
            .or_else(|| labels.and_then(|labels| labels.get("beta.kubernetes.io/os").cloned()));

        // Extract kubelet version
        let kubelet_version = node
            .status
            .as_ref()
            .and_then(|status| status.node_info.as_ref().map(|info| info.kubelet_version.clone()));

        nodes.push(NodeInfo {
            name,
            roles,
            instance_type,
            arch,
            os,
            kubelet_version,
        });
    }

    Ok(nodes)
}

/// Extract node roles from labels
fn extract_roles(node: &Node) -> Vec<String> {
    let mut roles = Vec::new();

    if let Some(labels) = &node.metadata.labels {
        // Standard role labels
        if labels.get("node-role.kubernetes.io/control-plane").is_some()
            || labels.get("node-role.kubernetes.io/master").is_some()
        {
            roles.push("control-plane".to_string());
        }
        if labels.get("node-role.kubernetes.io/worker").is_some() {
            roles.push("worker".to_string());
        }
        if labels.get("node-role.kubernetes.io/infra").is_some() {
            roles.push("infra".to_string());
        }

        // Legacy labels
        if labels.get("kubernetes.io/role").is_some_and(|r| r == "master")
            && !roles.contains(&"control-plane".to_string())
        {
            roles.push("control-plane".to_string());
        }
    }

    // If no roles found, add "node" as default
    if roles.is_empty() {
        roles.push("node".to_string());
    }

    roles
}

/// Detect cluster distribution from labels and API groups
async fn detect_distribution(client: &Client) -> Result<String, DiagError> {
    // Check for GKE labels
    let api: Api<Node> = Api::all(client.clone());
    let node_list = api
        .list(&Default::default())
        .await
        .map_err(DiagError::KubeError)?;

    for node in &node_list.items {
        if let Some(labels) = &node.metadata.labels {
            if labels.contains_key("cloud.google.com/machine-type") {
                return Ok("GKE".to_string());
            }
            if labels.contains_key("eks.amazonaws.com/nodegroup") {
                return Ok("EKS".to_string());
            }
            if labels.contains_key("kubernetes.azure.com/agentpool") {
                return Ok("AKS".to_string());
            }
        }
    }

    // OpenShift exposes its own API groups (e.g. *.openshift.io). Best-effort discovery.
    if let Ok(groups) = client.list_api_groups().await
        && groups.groups.iter().any(|g| g.name.ends_with("openshift.io"))
    {
        return Ok("OpenShift".to_string());
    }

    Ok("Unknown".to_string())
}

/// Get API server version
async fn get_apiserver_version(client: &Client) -> Result<String, DiagError> {
    let version = client.apiserver_version().await.map_err(DiagError::KubeError)?;
    Ok(version.git_version)
}

/// Get vynil controller version from deployment in vynil namespace
async fn get_vynil_version(client: &Client, vynil_namespace: &str) -> Result<Option<String>, DiagError> {
    use k8s_openapi::api::apps::v1::Deployment;

    let api: Api<Deployment> = Api::namespaced(client.clone(), vynil_namespace);
    match api.list(&Default::default()).await {
        Ok(list) => {
            for deployment in list.items {
                if let Some(labels) = &deployment.metadata.labels
                    && labels.get("app.kubernetes.io/name").is_some_and(|v| v == "vynil")
                {
                    // Found the vynil controller deployment
                    let image: Option<String> = if let Some(spec) = deployment.spec {
                        let template = spec.template;
                        if let Some(pod_spec) = template.spec {
                            pod_spec
                                .containers
                                .into_iter()
                                .next()
                                .and_then(|container| container.image)
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    if let Some(image) = image {
                        // Extract version from image tag
                        if let Some(tag) = image.as_str().split(':').nth(1) {
                            return Ok(Some(tag.to_string()));
                        }
                    }
                    break;
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to get vynil version: {}", e);
        }
    }

    Ok(None)
}

/// Get storage classes
async fn get_storage_classes(client: &Client) -> Result<Vec<StorageClassInfo>, DiagError> {
    let api: Api<StorageClass> = Api::all(client.clone());
    let sc_list = api
        .list(&Default::default())
        .await
        .map_err(DiagError::KubeError)?;

    let mut storage_classes = Vec::new();
    for sc in sc_list.items {
        let name = sc.metadata.name.unwrap_or_default();
        let provisioner = sc.provisioner;
        let is_default = sc
            .metadata
            .annotations
            .as_ref()
            .and_then(|annotations| annotations.get("storageclass.kubernetes.io/is-default-class"))
            .is_some_and(|v| v == "true");

        storage_classes.push(StorageClassInfo {
            name,
            provisioner,
            is_default,
        });
    }

    Ok(storage_classes)
}

/// Get ingress classes
async fn get_ingress_classes(client: &Client) -> Result<Vec<String>, DiagError> {
    let api: Api<IngressClass> = Api::all(client.clone());
    let ic_list = api
        .list(&Default::default())
        .await
        .map_err(DiagError::KubeError)?;

    Ok(ic_list
        .items
        .into_iter()
        .filter_map(|ic| ic.metadata.name)
        .collect())
}
