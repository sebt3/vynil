use crate::{auth::Identity, error::DiagError};
use k8s_openapi::api::authorization::v1::SubjectAccessReview;
use kube::{Api, Client};

/// Check if the identity has permission to access the instance resource
///
/// For instance-scoped items (state, children, agentlog, childlogs, operatorlog),
/// we need to verify the caller can read the instance via SubjectAccessReview.
///
/// For generic items (clusterinfo, vynilconfig, packages), no SAR is needed.
pub async fn check_instance_access(
    client: &Client,
    identity: &Identity,
    kind: &str,
    namespace: &str,
    name: &str,
) -> Result<bool, DiagError> {
    // Map kind to plural resource name
    let resource = match kind {
        "tenantinstances" => "tenantinstances",
        "serviceinstances" => "serviceinstances",
        "systeminstances" => "systeminstances",
        _ => return Err(DiagError::UnknownKind),
    };

    let sar_api: Api<SubjectAccessReview> = Api::all(client.clone());

    let sar = SubjectAccessReview {
        spec: k8s_openapi::api::authorization::v1::SubjectAccessReviewSpec {
            user: Some(identity.user.clone()),
            groups: Some(identity.groups.clone()),
            resource_attributes: Some(k8s_openapi::api::authorization::v1::ResourceAttributes {
                verb: Some("get".to_string()),
                group: Some("vynil.solidite.fr".to_string()),
                resource: Some(resource.to_string()),
                namespace: Some(namespace.to_string()),
                name: Some(name.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };

    let result = sar_api.create(&Default::default(), &sar).await.map_err(|e| {
        tracing::error!("SubjectAccessReview failed: {}", e);
        DiagError::InternalError(format!("SubjectAccessReview failed: {}", e))
    })?;

    Ok(result.status.as_ref().is_some_and(|s| s.allowed))
}

/// Check if the item requires instance-scoped authorization
pub fn is_instance_scoped_item(item: &str) -> bool {
    matches!(
        item,
        "state" | "children" | "agentlog" | "childlogs" | "operatorlog"
    )
}

/// Check authorization for a given item
///
/// Returns Ok(()) if authorized, or DiagError::AuthorizationDenied if not.
pub async fn check_item_access(
    client: &Client,
    identity: &Identity,
    kind: &str,
    namespace: &str,
    name: &str,
    item: &str,
    packages_enabled: bool,
) -> Result<(), DiagError> {
    // For instance-scoped items, perform SAR check
    if is_instance_scoped_item(item) {
        let allowed = check_instance_access(client, identity, kind, namespace, name).await?;
        if !allowed {
            return Err(DiagError::AuthorizationDenied);
        }
    }
    // For packages item, check if it's enabled
    else if item == "packages" && !packages_enabled {
        return Err(DiagError::PackagesDisabled);
    }

    // Generic items (clusterinfo, vynilconfig) or enabled packages don't require SAR
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_instance_scoped_item() {
        assert!(is_instance_scoped_item("state"));
        assert!(is_instance_scoped_item("children"));
        assert!(is_instance_scoped_item("agentlog"));
        assert!(is_instance_scoped_item("childlogs"));
        assert!(is_instance_scoped_item("operatorlog"));

        assert!(!is_instance_scoped_item("clusterinfo"));
        assert!(!is_instance_scoped_item("vynilconfig"));
        assert!(!is_instance_scoped_item("packages"));
    }
}
