use axum::Json;
use serde::{Deserialize, Serialize};

/// API group and version for the diagnostic API
pub const API_GROUP: &str = "diag.vynil.solidite.fr";
pub const API_VERSION: &str = "v1";

/// API resource definition for discovery
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct APIResource {
    name: String,
    namespaced: bool,
    kind: String,
}

/// API resource list response
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct APIResourceList {
    group: String,
    version: String,
    resources: Vec<APIResource>,
}

/// Handle the discovery endpoint
pub async fn discovery_handler() -> Json<APIResourceList> {
    Json(APIResourceList {
        group: API_GROUP.to_string(),
        version: API_VERSION.to_string(),
        resources: vec![
            APIResource {
                name: "tenantinstances".to_string(),
                namespaced: true,
                kind: "TenantInstance".to_string(),
            },
            APIResource {
                name: "serviceinstances".to_string(),
                namespaced: true,
                kind: "ServiceInstance".to_string(),
            },
            APIResource {
                name: "systeminstances".to_string(),
                namespaced: true,
                kind: "SystemInstance".to_string(),
            },
        ],
    })
}
