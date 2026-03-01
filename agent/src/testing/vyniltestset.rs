use common::httpmock::HttpMockItem;
use rhai::Dynamic;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Vynil TestSet
#[allow(non_snake_case)]
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct VynilTestSet {
    pub apiVersion: String,
    pub kind: String,
    /// Metadata for a testSet
    pub metadata: VynilTestSetMeta,
    /// Variables for a testSet
    pub variables: Option<VynilTestSetVariablesSet>,
    /// Mock definitions (kubernetes objects and http endpoints)
    pub mocks: Option<VynilTestSetMocks>,
    /// Assert definitions
    pub asserts: Option<Vec<VynilAssert>>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct VynilTestSetMeta {
    pub name: String,
    pub description: Option<String>,
}

/// Map of variable name → variable definition
pub type VynilTestSetVariablesSet = BTreeMap<String, VynilTestSetVariable>;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct VynilTestSetVariable {
    #[serde(rename = "type")]
    pub var_type: String,
    pub default: Option<serde_json::Value>,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum VynilAssertMatch {
    /// Exactly <count> of the selected objects should match the value
    Exact(u64),
    /// At least <count> of the selected objects should match the value
    AtLeast(u64),
    /// At most <count> of the selected objects match the value
    AtMost(u64),
    /// All selected objects should match the value
    #[default]
    All,
    /// One of the selected objects should match the value
    Any,
    /// None of the selected objects should match the value
    None,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct VynilAssertSelector {
    /// Select by kind
    pub kind: Option<String>,
    /// Select by name
    pub name: Option<String>,
    /// Select by namespace
    pub namespace: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct VynilAssert {
    /// Assert name
    pub name: String,
    /// Assert description
    pub description: Option<String>,
    /// Select the returned objects to assert
    pub selector: VynilAssertSelector,
    /// Match definition
    pub matcher: VynilAssertMatch,
    /// The expected kubernetes object values to validate
    pub value: Option<serde_json::Value>,
}

#[derive(Clone, Debug)]
pub struct VynilAssertResult {
    pub name: String,
    pub description: Option<String>,
    pub passed: bool,
    pub message: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct VynilTestSetMocks {
    /// HTTP endpoint mocks
    pub http: Option<Vec<HttpMockItem>>,
    /// Kubernetes object mocks
    pub kubernetes: Option<Vec<Dynamic>>,
}
