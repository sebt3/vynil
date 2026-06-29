use common::Children;
use serde::{Deserialize, Serialize};

/// Cluster information DTO
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClusterInfo {
    pub nodes: Vec<NodeInfo>,
    pub distribution: String,
    pub kubernetes_version: String,
    pub vynil_version: Option<String>,
    pub storage_classes: Vec<StorageClassInfo>,
    pub ingress_classes: Vec<String>,
}

/// Node information
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NodeInfo {
    pub name: String,
    pub roles: Vec<String>,
    pub instance_type: Option<String>,
    pub arch: Option<String>,
    pub os: Option<String>,
    pub kubelet_version: Option<String>,
}

/// Storage class information
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StorageClassInfo {
    pub name: String,
    pub provisioner: String,
    pub is_default: bool,
}

/// Package state DTO
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PackagesState {
    pub items: Vec<PackageState>,
}

/// Individual package state
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PackageState {
    pub kind: String,
    pub namespace: String,
    pub name: String,
    pub package: String,
    pub tag: Option<String>,
    pub ready: bool,
}

/// Child information with current state
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChildWithState {
    pub child: Children,
    pub state: Option<serde_json::Value>,
}

/// Scrub statistics for anonymization
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ScrubStats {
    pub distinct: usize,
    pub occurrences: usize,
}

/// Response envelope for anonymized content
#[derive(Serialize, Debug, Clone)]
pub struct AnonymizedResponse<T> {
    pub data: T,
    pub scrub_stats: ScrubStats,
}
