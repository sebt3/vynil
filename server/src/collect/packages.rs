use crate::{
    dto::{PackageState, PackagesState},
    error::DiagError,
};
use common::{instanceservice::ServiceInstance, instancesystem::SystemInstance};
use kube::{Api, Client};

/// Get the cluster-wide packages state.
///
/// SECURITY: this deliberately enumerates only the **platform** layer — `ServiceInstance` and
/// `SystemInstance` — and never `TenantInstance`. Listing tenant instances cluster-wide leaks
/// every other tenant's workloads to any caller who can reach the diag API for a single instance
/// they own. The platform inventory is the legitimate remote-debug context; it stays gated behind
/// the `diagnostic_expose_packages` option (the admin opt-out for stricter setups).
pub async fn get_packages(client: &Client) -> Result<PackagesState, DiagError> {
    let mut items = Vec::new();

    // Get ServiceInstance packages
    items.extend(get_service_packages(client).await?);

    // Get SystemInstance packages
    items.extend(get_system_packages(client).await?);

    Ok(PackagesState { items })
}

/// Get packages from ServiceInstance resources
async fn get_service_packages(client: &Client) -> Result<Vec<PackageState>, DiagError> {
    let api: Api<ServiceInstance> = Api::all(client.clone());
    let list = api
        .list(&Default::default())
        .await
        .map_err(DiagError::KubeError)?;

    Ok(list
        .items
        .into_iter()
        .filter_map(|instance| {
            let metadata = instance.metadata;
            let namespace = metadata.namespace?;
            let name = metadata.name?;
            let spec = instance.spec;
            let status = instance.status?;

            Some(PackageState {
                kind: "ServiceInstance".to_string(),
                namespace,
                name,
                package: spec.package,
                tag: status.tag.clone(),
                ready: is_ready_service(&status),
            })
        })
        .collect())
}

/// Get packages from SystemInstance resources
async fn get_system_packages(client: &Client) -> Result<Vec<PackageState>, DiagError> {
    let api: Api<SystemInstance> = Api::all(client.clone());
    let list = api
        .list(&Default::default())
        .await
        .map_err(DiagError::KubeError)?;

    Ok(list
        .items
        .into_iter()
        .filter_map(|instance| {
            let metadata = instance.metadata;
            let namespace = metadata.namespace?;
            let name = metadata.name?;
            let spec = instance.spec;
            let status = instance.status?;

            Some(PackageState {
                kind: "SystemInstance".to_string(),
                namespace,
                name,
                package: spec.package,
                tag: status.tag.clone(),
                ready: is_ready_system(&status),
            })
        })
        .collect())
}

/// Check if an instance status has Ready=True condition
fn is_ready_service(status: &common::instanceservice::ServiceInstanceStatus) -> bool {
    for condition in &status.conditions {
        if condition.condition_type == common::instanceservice::ConditionsType::Ready {
            return condition.status == common::instanceservice::ConditionsStatus::True;
        }
    }
    false
}

/// Check if an instance status has Ready=True condition
fn is_ready_system(status: &common::instancesystem::SystemInstanceStatus) -> bool {
    for condition in &status.conditions {
        if condition.condition_type == common::instancesystem::ConditionsType::Ready {
            return condition.status == common::instancesystem::ConditionsStatus::True;
        }
    }
    false
}
