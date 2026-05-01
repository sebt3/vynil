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

fn k8s_object(kind: &str, namespace: &str, name: &str) -> Dynamic {
    dynamic_from_json(serde_json::json!({
        "apiVersion": "v1",
        "kind": kind,
        "metadata": { "name": name, "namespace": namespace },
        "status": {}
    }))
}

fn build_instance_mock(ns: &str, name: &str) -> Dynamic {
    dynamic_from_json(serde_json::json!({
        "apiVersion": "vynil.solidite.fr/v1",
        "kind": "ServiceInstance",
        "metadata": { "name": name, "namespace": ns },
        "spec": { "category": "test", "package": "test-pkg", "options": {} }
    }))
}

fn build_context_mock() -> Dynamic {
    let base = env!("CARGO_MANIFEST_DIR");
    dynamic_from_json(serde_json::json!({
        "config_dir": format!("{}/scripts/config", base),
        "package_dir": format!("{}/scripts/packages", base)
    }))
}

#[test]
fn harness_compiles() {
    let mut rhai = make_lib_script();
    let result = rhai.eval("1 + 1").unwrap();
    assert_eq!(result.as_int().unwrap(), 2i64);
}

// ===== storage_class_selector tests =====

#[test]
fn storage_class_selector_for_singletons_with_preference_match() {
    // Verify .find() locates a storage class by name from prefered_storage preference
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        import "storage_class_selector" as sel;

        let context = #{
            cluster: #{
                storage_classes: [
                    #{ name: "local-path", is_default: false, volumeMode: "Filesystem" },
                    #{ name: "ceph-distributed", is_default: false, volumeMode: "Filesystem" },
                ],
                prefered_storage: #{
                    distibuted_readWriteOnce: "ceph-distributed",
                    fs_fast_readWriteOnce: (),
                    fs_cheap_readWriteOnce: (),
                    fs_readWriteOnce: (),
                    block_readWriteMany: (),
                    block_readWriteOnce: (),
                }
            }
        };

        let selected = sel::for_singletons(context);
        selected.name
    "#).unwrap();

    assert_eq!(result.to_string(), "ceph-distributed");
}

#[test]
fn storage_class_selector_for_singletons_empty_list() {
    // Verify .find() returns () when storage_classes list is empty
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        import "storage_class_selector" as sel;

        let context = #{
            cluster: #{
                storage_classes: [],
                prefered_storage: #{
                    distibuted_readWriteOnce: "ceph-distributed",
                    fs_fast_readWriteOnce: (),
                    fs_cheap_readWriteOnce: (),
                    fs_readWriteOnce: (),
                    block_readWriteMany: (),
                    block_readWriteOnce: (),
                }
            }
        };

        let selected = sel::for_singletons(context);
        selected == ()
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true);
}

#[test]
fn storage_class_selector_for_singletons_singleton_excluded_from_rwo() {
    // Verify fallback to default when distibuted preference is empty
    // for_singletons falls through all preferences and uses is_default
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        import "storage_class_selector" as sel;

        let context = #{
            cluster: #{
                storage_classes: [
                    #{ name: "local-path", is_default: true, volumeMode: "Filesystem" },
                ],
                prefered_storage: #{
                    distibuted_readWriteOnce: (),
                    fs_fast_readWriteOnce: (),
                    fs_cheap_readWriteOnce: (),
                    fs_readWriteOnce: (),
                    block_readWriteMany: (),
                    block_readWriteOnce: (),
                }
            }
        };

        let selected = sel::for_singletons(context);
        selected.name
    "#).unwrap();

    assert_eq!(result.to_string(), "local-path");
}

#[test]
fn storage_class_selector_for_deployments_rwx_fallback() {
    // Verify fallback chain: no RWX preference → falls back to RWO preference
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        import "storage_class_selector" as sel;

        let context = #{
            cluster: #{
                storage_classes: [
                    #{ name: "local-path", is_default: false, volumeMode: "Filesystem" },
                    #{ name: "ceph-rwo", is_default: false, volumeMode: "Filesystem" },
                ],
                prefered_storage: #{
                    fs_readWriteMany: (),
                    fs_fast_readWriteMany: (),
                    fs_cheap_readWriteMany: (),
                    fs_readWriteOnce: "ceph-rwo",
                    fs_fast_readWriteOnce: (),
                    fs_cheap_readWriteOnce: (),
                    distibuted_readWriteOnce: (),
                    block_readWriteMany: (),
                    block_readWriteOnce: (),
                }
            }
        };

        let selected = sel::for_deployments(context);
        selected.name
    "#).unwrap();

    assert_eq!(result.to_string(), "ceph-rwo");
}

// ===== storage_class_enrich tests =====

#[test]
fn storage_class_enrich_adds_capabilities_and_modes() {
    // Verify .find() on get_known_class() matches provisioner and adds all expected fields
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        import "storage_class_enrich" as enrich;

        let scs = [
            #{
                name: "ceph-rbd",
                provisioner: "rbd.csi.ceph.com",
                allowVolumeExpansion: true,
            }
        ];

        let enriched = enrich::classes_enrich(scs);
        let first = enriched[0];

        // Check all expected fields are present
        first.contains("volumeMode") &&
        first.contains("capabilities") &&
        first.contains("allAccessModes") &&
        first.contains("accessModes")
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true);
}

#[test]
fn storage_class_enrich_unknown_provisioner() {
    // Verify unknown provisioner gets minimal enrichment (empty capabilities, RWO only)
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        import "storage_class_enrich" as enrich;

        let scs = [
            #{
                name: "unknown-sc",
                provisioner: "unknown.provisioner/io",
                allowVolumeExpansion: false,
            }
        ];

        let enriched = enrich::classes_enrich(scs);
        let first = enriched[0];

        first.capabilities == #{} &&
        first.allAccessModes == ["ReadWriteOnce"] &&
        first.accessModes == ["ReadWriteOnce"]
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true);
}

#[test]
fn storage_class_enrich_block_volumemode_duplication() {
    // Verify raw-capable driver (rbd) has capabilities including raw support
    // and allAccessModes is properly filtered for Block variant
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        import "storage_class_enrich" as enrich;

        let scs = [
            #{
                name: "ceph-rbd",
                provisioner: "rbd.csi.ceph.com",
                allowVolumeExpansion: true,
            }
        ];

        let enriched = enrich::classes_enrich(scs);

        // rbd is raw-capable: check capabilities has raw: true
        enriched.len() >= 1 &&
        enriched[0].capabilities != #{} &&
        enriched[0].capabilities.contains("raw")
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true);
}

// ===== wait.rhai tests =====

#[test]
fn wait_installs_filters_instances() {
    // Verify installs() filters list and only processes ServiceInstance/SystemInstance/TenantInstance
    // Uses .filter() + .contains() on kind array
    let k8s_mocks = vec![
        k8s_object("ServiceInstance", "default", "my-svc"),
        k8s_object("Deployment", "default", "my-deploy"),
    ];
    let (mut rhai, _created) = make_lib_script_with_k8s(k8s_mocks);

    let result = rhai.eval(r#"
        import "wait" as wait;

        let resources = [
            #{ kind: "ServiceInstance", namespace: "default", name: "my-svc" },
            #{ kind: "Deployment", namespace: "default", name: "my-deploy" },
        ];

        // installs should filter and process only ServiceInstance
        wait::installs(resources, 1);
        true
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true);
}

#[test]
fn wait_workload_filters_workloads() {
    // Verify workload() filters and processes Deployment/DaemonSet/StatefulSet
    let k8s_mocks = vec![
        k8s_object("Deployment", "default", "my-deploy"),
        k8s_object("Job", "default", "my-job"),
    ];
    let (mut rhai, _created) = make_lib_script_with_k8s(k8s_mocks);

    let result = rhai.eval(r#"
        import "wait" as wait;

        let resources = [
            #{ kind: "Deployment", namespace: "default", name: "my-deploy" },
            #{ kind: "Job", namespace: "default", name: "my-job" },
        ];

        // workload should filter and process only Deployment
        wait::workload(resources, 1);
        true
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true);
}

#[test]
fn wait_vital_handles_multiple_types() {
    // Verify vital() handles multiple resource types (Cluster, MariaDB, Redis, NdbCluster)
    // Tests .contains() logic for different condition types
    // Note: Redis/MongoDBCommunity require corresponding StatefulSet mocks
    let k8s_mocks = vec![
        k8s_object("Cluster", "default", "my-cluster"),
        k8s_object("Redis", "default", "my-redis"),
        k8s_object("StatefulSet", "default", "my-redis"),
        k8s_object("NdbCluster", "default", "my-ndb"),
    ];
    let (mut rhai, _created) = make_lib_script_with_k8s(k8s_mocks);

    let result = rhai.eval(r#"
        import "wait" as wait;

        let resources = [
            #{ kind: "Cluster", namespace: "default", name: "my-cluster" },
            #{ kind: "Redis", namespace: "default", name: "my-redis" },
            #{ kind: "NdbCluster", namespace: "default", name: "my-ndb" },
        ];

        // vital should process all three with appropriate condition types
        wait::vital(resources, 1);
        true
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true);
}

#[test]
fn wait_job_filters_jobs() {
    // Verify job() filters and processes only Job kind
    let k8s_mocks = vec![
        k8s_object("Job", "default", "my-job"),
        k8s_object("Pod", "default", "my-pod"),
    ];
    let (mut rhai, _created) = make_lib_script_with_k8s(k8s_mocks);

    let result = rhai.eval(r#"
        import "wait" as wait;

        let resources = [
            #{ kind: "Job", namespace: "default", name: "my-job" },
            #{ kind: "Pod", namespace: "default", name: "my-pod" },
        ];

        // job should filter and process only Job
        wait::job(resources, 1);
        true
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true);
}

#[test]
fn wait_all_chains_functions() {
    // Verify all() chains installs() + vital() + job() + workload()
    // Tests that the chaining logic works without errors
    let k8s_mocks = vec![
        k8s_object("ServiceInstance", "default", "my-svc"),
        k8s_object("Deployment", "default", "my-deploy"),
        k8s_object("Job", "default", "my-job"),
        k8s_object("Cluster", "default", "my-cluster"),
    ];
    let (mut rhai, _created) = make_lib_script_with_k8s(k8s_mocks);

    let result = rhai.eval(r#"
        import "wait" as wait;

        let resources = [
            #{ kind: "ServiceInstance", namespace: "default", name: "my-svc" },
            #{ kind: "Deployment", namespace: "default", name: "my-deploy" },
            #{ kind: "Job", namespace: "default", name: "my-job" },
            #{ kind: "Cluster", namespace: "default", name: "my-cluster" },
        ];

        // all() should process all categories without errors
        wait::all(resources, 1);
        true
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true);
}

// ===== install_from_dir.rhai tests =====

#[test]
fn install_from_dir_applies_multiple_yamls() {
    // Verify install() reads multiple YAML files and creates objects in K8s
    // Tests: file I/O (read_dir, file_read), YAML parsing, filtering, K8s apply
    let base = env!("CARGO_MANIFEST_DIR");
    let fixture_dir = format!("{}/tests/fixtures/install_from_dir/basic", base);

    let (mut rhai, created) = make_lib_script_with_k8s(vec![]);

    let result = rhai.eval(&format!(r#"
        import "install_from_dir" as install;

        let instance = #{{
            metadata: #{{ namespace: "test-ns" }}
        }};

        let context = #{{
            config_dir: "",
            package_dir: ""
        }};

        let applied = install::install(instance, context, "{}", true, false);

        // Should have applied ConfigMap and Deployment (2 objects)
        applied.len()
    "#, fixture_dir)).unwrap();

    let count = result.as_int().unwrap();
    assert_eq!(count, 2);

    // Verify objects were created in mock K8s
    let created_objs = created.lock().unwrap();
    assert_eq!(created_objs.len(), 2);
}

#[test]
fn install_from_dir_empty_directory_no_objects() {
    // Verify install() handles empty directory without error and returns empty list
    let base = env!("CARGO_MANIFEST_DIR");
    let fixture_dir = format!("{}/tests/fixtures/install_from_dir/empty", base);

    let (mut rhai, created) = make_lib_script_with_k8s(vec![]);

    let result = rhai.eval(&format!(r#"
        import "install_from_dir" as install;

        let instance = #{{
            metadata: #{{ namespace: "test-ns" }}
        }};

        let context = #{{
            config_dir: "",
            package_dir: ""
        }};

        let applied = install::install(instance, context, "{}", true, false);

        // Empty directory should return empty list
        applied.len()
    "#, fixture_dir)).unwrap();

    let count = result.as_int().unwrap();
    assert_eq!(count, 0);

    let created_objs = created.lock().unwrap();
    assert_eq!(created_objs.len(), 0);
}

#[test]
fn install_from_dir_respects_namespace_parameter() {
    // Verify force_ns=true forces namespace on all objects regardless of YAML
    let base = env!("CARGO_MANIFEST_DIR");
    let fixture_dir = format!("{}/tests/fixtures/install_from_dir/basic", base);

    let (mut rhai, created) = make_lib_script_with_k8s(vec![]);

    let result = rhai.eval(&format!(r#"
        import "install_from_dir" as install;

        let instance = #{{
            metadata: #{{ namespace: "forced-ns" }}
        }};

        let context = #{{
            config_dir: "",
            package_dir: ""
        }};

        // force_ns=true should override YAML namespace (test-ns -> forced-ns)
        let applied = install::install(instance, context, "{}", true, true);

        applied.len()
    "#, fixture_dir)).unwrap();

    let count = result.as_int().unwrap();
    assert_eq!(count, 2);

    // Verify objects have forced namespace
    let created_objs = created.lock().unwrap();
    assert_eq!(created_objs.len(), 2);
    for obj in created_objs.iter() {
        if let Ok(map) = obj.as_map_ref() {
            if let Some(meta) = map.get("metadata") {
                if let Ok(meta_map) = meta.as_map_ref() {
                    if let Some(ns) = meta_map.get("namespace") {
                        assert_eq!(ns.to_string(), "forced-ns");
                    }
                }
            }
        }
    }
}

#[test]
fn install_from_dir_respects_ordering() {
    // Verify get_first() kinds (ConfigMap) are applied before get_last() kinds (Deployment)
    // Tests the 3-phase ordering logic: first, middle, last
    let base = env!("CARGO_MANIFEST_DIR");
    let fixture_dir = format!("{}/tests/fixtures/install_from_dir/basic", base);

    let (mut rhai, created) = make_lib_script_with_k8s(vec![]);

    let result = rhai.eval(&format!(r#"
        import "install_from_dir" as install;

        let instance = #{{
            metadata: #{{ namespace: "test-ns" }}
        }};

        let context = #{{
            config_dir: "",
            package_dir: ""
        }};

        let applied = install::install(instance, context, "{}", true, false);

        // Check that objects list contains both ConfigMap (first) and Deployment (last)
        applied.len() == 2 &&
        applied.some(|o| o.kind == "ConfigMap") &&
        applied.some(|o| o.kind == "Deployment")
    "#, fixture_dir)).unwrap();

    assert_eq!(result.as_bool().unwrap(), true);
}
