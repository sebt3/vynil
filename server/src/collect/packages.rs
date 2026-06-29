use crate::{
    dto::{PackageState, PackagesState},
    error::DiagError,
};
use common::{
    instanceservice::ServiceInstance, instancesystem::SystemInstance, instancetenant::TenantInstance,
};
use kube::{Api, Client};

/// Get packages state from all namespaces
pub async fn get_packages(client: &Client) -> Result<PackagesState, DiagError> {
    let mut items = Vec::new();

    // Get TenantInstance packages
    items.extend(get_tenant_packages(client).await?);

    // Get ServiceInstance packages
    items.extend(get_service_packages(client).await?);

    // Get SystemInstance packages
    items.extend(get_system_packages(client).await?);

    Ok(PackagesState { items })
}

/// Get packages from TenantInstance resources
async fn get_tenant_packages(client: &Client) -> Result<Vec<PackageState>, DiagError> {
    let api: Api<TenantInstance> = Api::all(client.clone());
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
                kind: "TenantInstance".to_string(),
                namespace,
                name,
                package: spec.package,
                tag: status.tag.clone(),
                ready: is_ready_tenant(&status),
            })
        })
        .collect())
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
fn is_ready_tenant(status: &common::instancetenant::TenantInstanceStatus) -> bool {
    for condition in &status.conditions {
        if condition.condition_type == common::instancetenant::ConditionsType::Ready {
            return condition.status == common::instancetenant::ConditionsStatus::True;
        }
    }
    false
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
