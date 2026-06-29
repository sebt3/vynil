use std::collections::HashMap;

/// All known diagnostic items in collection order.
pub const ALL_ITEMS: &[&str] = &[
    "clusterinfo",
    "vynilconfig",
    "packages",
    "state",
    "children",
    "agentlog",
    "childlogs",
    "operatorlog",
];

/// Returns the list of items to collect, respecting the optional filter.
pub fn resolve_items(filter: Option<&[String]>) -> Vec<&'static str> {
    match filter {
        Some(items) if !items.is_empty() => {
            let requested: HashMap<&str, ()> = items.iter().map(|s| (s.as_str(), ())).collect();
            ALL_ITEMS
                .iter()
                .filter(|item| requested.contains_key(*item))
                .copied()
                .collect()
        }
        _ => ALL_ITEMS.to_vec(),
    }
}

/// Maps an item name to its path within the bundle.
/// The extension will be appended based on Content-Type.
pub fn item_path(item: &str) -> &'static str {
    match item {
        "clusterinfo" => "cluster/clusterinfo",
        "vynilconfig" => "config/vynilconfig",
        "packages" => "packages/packages",
        "state" => "instance/state",
        "children" => "instance/children",
        "agentlog" => "logs/agentlog",
        "childlogs" => "logs/childlogs",
        "operatorlog" => "logs/operatorlog",
        _ => panic!("unknown item: {}", item),
    }
}

/// Maps Content-Type to file extension.
pub fn extension_for_content_type(content_type: &str) -> &'static str {
    let ct = content_type.to_lowercase();
    if ct.contains("application/json") {
        ".json"
    } else if ct.contains("application/yaml") || ct.contains("text/yaml") {
        ".yaml"
    } else if ct.contains("text/plain") {
        ".log"
    } else {
        ".txt"
    }
}

/// Builds the full API path for a given item.
pub fn api_path(target: &crate::cli::InstanceTarget, item: &str) -> String {
    format!(
        "/apis/diag.vynil.solidite.fr/v1/namespaces/{}/{}/{}/{}",
        target.namespace, target.kind, target.name, item
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_items_all() {
        let items = resolve_items(None);
        assert_eq!(items, ALL_ITEMS);
    }

    #[test]
    fn test_resolve_items_filtered() {
        let filter = vec!["state".to_string(), "agentlog".to_string()];
        let items = resolve_items(Some(&filter));
        assert_eq!(items, vec!["state", "agentlog"]);
    }

    #[test]
    fn test_resolve_items_empty_filter() {
        let filter: Vec<String> = vec![];
        let items = resolve_items(Some(&filter));
        assert_eq!(items, ALL_ITEMS);
    }

    #[test]
    fn test_item_paths() {
        assert_eq!(item_path("clusterinfo"), "cluster/clusterinfo");
        assert_eq!(item_path("vynilconfig"), "config/vynilconfig");
        assert_eq!(item_path("packages"), "packages/packages");
        assert_eq!(item_path("state"), "instance/state");
        assert_eq!(item_path("children"), "instance/children");
        assert_eq!(item_path("agentlog"), "logs/agentlog");
        assert_eq!(item_path("childlogs"), "logs/childlogs");
        assert_eq!(item_path("operatorlog"), "logs/operatorlog");
    }

    #[test]
    fn test_extension_for_content_type() {
        assert_eq!(extension_for_content_type("application/json"), ".json");
        assert_eq!(
            extension_for_content_type("application/json; charset=utf-8"),
            ".json"
        );
        assert_eq!(extension_for_content_type("application/yaml"), ".yaml");
        assert_eq!(extension_for_content_type("text/yaml"), ".yaml");
        assert_eq!(extension_for_content_type("text/plain"), ".log");
        assert_eq!(extension_for_content_type("text/plain; charset=utf-8"), ".log");
        assert_eq!(extension_for_content_type("application/octet-stream"), ".txt");
    }

    #[test]
    fn test_api_path() {
        let target = crate::cli::InstanceTarget {
            namespace: "kydah-core".to_string(),
            kind: "systeminstances".to_string(),
            name: "reloader".to_string(),
        };
        assert_eq!(
            api_path(&target, "clusterinfo"),
            "/apis/diag.vynil.solidite.fr/v1/namespaces/kydah-core/systeminstances/reloader/clusterinfo"
        );
    }
}
