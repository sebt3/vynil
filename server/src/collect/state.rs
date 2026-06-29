use crate::{anonymize::scrub_yaml, dto::ScrubStats, error::DiagError};
use common::{
    instanceservice::ServiceInstance, instancesystem::SystemInstance, instancetenant::TenantInstance,
};
use kube::{Api, Client};
use serde_yaml;

/// Get instance state for a specific instance
pub async fn get_instance_state(
    client: &Client,
    kind: &str,
    namespace: &str,
    name: &str,
    vynil_namespace: &str,
) -> Result<(String, ScrubStats), DiagError> {
    let instance_json = match kind {
        "tenantinstances" => {
            let instance = get_tenant_instance(client, namespace, name).await?;
            serde_json::to_value(instance).map_err(DiagError::SerializationError)?
        }
        "serviceinstances" => {
            let instance = get_service_instance(client, namespace, name).await?;
            serde_json::to_value(instance).map_err(DiagError::SerializationError)?
        }
        "systeminstances" => {
            let instance = get_system_instance(client, namespace, name).await?;
            serde_json::to_value(instance).map_err(DiagError::SerializationError)?
        }
        _ => return Err(DiagError::UnknownKind),
    };

    // Convert to YAML
    let yaml = serde_yaml::to_string(&instance_json).map_err(DiagError::YamlError)?;

    // Anonymize the YAML content
    let (scrubbed, stats) = scrub_yaml(&yaml, client, namespace, vynil_namespace).await;

    Ok((scrubbed, stats))
}

/// Get TenantInstance by name and namespace, stripping sensitive fields
async fn get_tenant_instance(
    client: &Client,
    namespace: &str,
    name: &str,
) -> Result<TenantInstance, DiagError> {
    let api: Api<TenantInstance> = Api::namespaced(client.clone(), namespace);
    let instance = api.get(name).await.map_err(DiagError::KubeError)?;

    // Strip sensitive fields (tfstate, rhaistate) as per brief §7.4
    let mut stripped = instance;
    if let Some(status) = &mut stripped.status {
        status.tfstate = None;
        status.rhaistate = None;
    }

    Ok(stripped)
}

/// Get ServiceInstance by name and namespace, stripping sensitive fields
async fn get_service_instance(
    client: &Client,
    namespace: &str,
    name: &str,
) -> Result<ServiceInstance, DiagError> {
    let api: Api<ServiceInstance> = Api::namespaced(client.clone(), namespace);
    let instance = api.get(name).await.map_err(DiagError::KubeError)?;

    // Strip sensitive fields (tfstate, rhaistate) as per brief §7.4
    let mut stripped = instance;
    if let Some(status) = &mut stripped.status {
        status.tfstate = None;
        status.rhaistate = None;
    }

    Ok(stripped)
}

/// Get SystemInstance by name and namespace, stripping sensitive fields
async fn get_system_instance(
    client: &Client,
    namespace: &str,
    name: &str,
) -> Result<SystemInstance, DiagError> {
    let api: Api<SystemInstance> = Api::namespaced(client.clone(), namespace);
    let instance = api.get(name).await.map_err(DiagError::KubeError)?;

    // Strip sensitive fields (tfstate, rhaistate) as per brief §7.4
    let mut stripped = instance;
    if let Some(status) = &mut stripped.status {
        status.tfstate = None;
        status.rhaistate = None;
    }

    Ok(stripped)
}
