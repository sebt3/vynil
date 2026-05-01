use common::rhaihandler::Script;
use rhai::Dynamic;
use std::sync::{Arc, Mutex};

pub fn make_service_script(
    k8s_mocks: Vec<Dynamic>,
) -> (Script, Arc<Mutex<Vec<Dynamic>>>) {
    let base = env!("CARGO_MANIFEST_DIR");
    let created = Arc::new(Mutex::new(vec![]));
    let script = Script::new_mock(
        vec![
            format!("{base}/scripts/service"),
            format!("{base}/scripts/lib"),
        ],
        vec![],
        k8s_mocks,
        created.clone(),
    );
    (script, created)
}

pub fn build_service_instance_mock(
    ns: &str, name: &str, category: &str, package: &str,
) -> Dynamic {
    let json = serde_json::json!({
        "apiVersion": "vynil.solidite.fr/v1",
        "kind": "ServiceInstance",
        "metadata": { "name": name, "namespace": ns },
        "spec": { "category": category, "package": package, "options": {} },
        "status": {}
    });
    serde_json::from_str(&serde_json::to_string(&json).unwrap()).unwrap()
}

pub fn build_args(ns: &str, instance: &str) -> serde_json::Value {
    let base = env!("CARGO_MANIFEST_DIR");
    serde_json::json!({
        "namespace": ns,
        "instance": instance,
        "vynil_namespace": "vynil-system",
        "package_dir": format!("{base}/tests/fixtures/package"),
        "script_dir": format!("{base}/scripts"),
        "template_dir": format!("{base}/templates"),
        "config_dir": format!("{base}/tests/fixtures/config"),
        "agent_image": "vynil-agent:test",
        "tag": "0.1.0",
        "controller_values": "{}",
    })
}

#[test]
fn harness_compiles() {
    let (mut rhai, _) = make_service_script(vec![]);
    let result = rhai.eval("1 + 1").unwrap();
    assert_eq!(result.as_int().unwrap(), 2i64);
}
