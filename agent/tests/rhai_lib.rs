use common::rhaihandler::Script;
use rhai::Dynamic;
use std::sync::{Arc, Mutex};

pub fn make_lib_script() -> Script {
    let base = env!("CARGO_MANIFEST_DIR");
    Script::new_mock(
        vec![format!("{base}/scripts/lib")],
        vec![],
        vec![],
        Default::default(),
    )
}

pub fn make_lib_script_with_k8s(
    k8s_mocks: Vec<Dynamic>,
) -> (Script, Arc<Mutex<Vec<Dynamic>>>) {
    let base = env!("CARGO_MANIFEST_DIR");
    let created = Arc::new(Mutex::new(vec![]));
    let script = Script::new_mock(
        vec![format!("{base}/scripts/lib")],
        vec![],
        k8s_mocks,
        created.clone(),
    );
    (script, created)
}

pub fn dynamic_from_json(json: serde_json::Value) -> Dynamic {
    serde_json::from_str(&serde_json::to_string(&json).unwrap()).unwrap()
}

#[test]
fn harness_compiles() {
    let mut rhai = make_lib_script();
    let result = rhai.eval("1 + 1").unwrap();
    assert_eq!(result.as_int().unwrap(), 2i64);
}
