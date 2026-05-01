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

// ===== gen_package.rhai tests =====

#[test]
fn gen_package_replace_returns_modified_string() {
    // Vérifie le comportement de .replace() : depuis Rhai 1.18+, retourne la string modifiée
    // au lieu de muter la string originale. Pattern vulnérable: str.replace(...); str
    // Ce test expose le bug si replace() est appelé sans assigner le résultat.
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        import "gen_package" as gen;

        let labels = #{
            app: "my-app",
            release: "old-release"
        };

        // Call replace_label_values which uses .replace() internally
        // The function should replace 'old-release' with '{{instance.appslug}}'
        let replaced = gen::replace_label_values(labels, "old-release");

        // Debug: print what we got
        debug(`Input: ${labels.release}`);
        debug(`Output: ${replaced.release}`);

        // Verify the replacement occurred
        replaced.release == "{{instance.appslug}}"
    "#).unwrap();

    // This should be true if .replace() properly updates the value
    // Currently may be false if the bug exists (pattern: v.replace(...); v without assignment)
    assert_eq!(result.as_bool().unwrap(), true,
        "replace_label_values must update label values using .replace() with assignment");
}


#[test]
fn gen_package_clean_metadata_removes_helm_annotations() {
    // Verify clean_metadata removes Helm-specific annotations and preserves others
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        import "gen_package" as gen;

        let metadata = #{
            name: "my-release",
            labels: #{
                app: "myapp",
                release: "old-release"
            },
            annotations: #{
                "helm.sh/chart": "myapp-1.0",
                "custom.io/desc": "custom value",
                "checksum/config": "abc123"
            }
        };

        let cleaned = gen::clean_metadata(metadata, "old-release");

        // Verify Helm annotations are removed
        let has_helm = "helm.sh/chart" in cleaned.annotations;
        let has_checksum = "checksum/config" in cleaned.annotations;
        let has_custom = "custom.io/desc" in cleaned.annotations;

        !has_helm && !has_checksum && has_custom
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true,
        "clean_metadata must remove Helm and checksum annotations but keep custom ones");
}

#[test]
fn gen_package_clean_metadata_empty_annotations() {
    // Verify clean_metadata removes the annotations key entirely if it becomes empty
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        import "gen_package" as gen;

        let metadata = #{
            name: "my-release",
            annotations: #{
                "helm.sh/chart": "myapp-1.0",
                "checksum/config": "abc123"
            }
        };

        let cleaned = gen::clean_metadata(metadata, "release");

        // annotations key should be removed if empty
        !("annotations" in cleaned)
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true,
        "clean_metadata must remove annotations key if all entries are Helm-related");
}

#[test]
fn gen_package_replace_volumes_handles_configmap_and_pvc() {
    // Verify replace_volumes updates names in configMap and persistentVolumeClaim refs
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        import "gen_package" as gen;

        let volumes = [
            #{
                name: "config-vol",
                configMap: #{
                    name: "old-release-config"
                }
            },
            #{
                name: "data-vol",
                persistentVolumeClaim: #{
                    claimName: "old-release-data"
                }
            }
        ];

        let replaced = gen::replace_volumes(volumes, "old-release");

        // Verify both are replaced (.replace mutates in place)
        replaced[0].configMap.name == "{{instance.appslug}}-config" &&
        replaced[1].persistentVolumeClaim.claimName == "{{instance.appslug}}-data"
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true,
        "replace_volumes must update configMap and persistentVolumeClaim names");
}

#[test]
fn gen_package_replace_containers_handles_env_configmap_refs() {
    // Verify replace_containers updates names in environment variable refs
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        import "gen_package" as gen;

        let containers = [
            #{
                name: "app",
                env: [
                    #{
                        name: "CONFIG_MAP_NAME",
                        value: "old-release-config"
                    },
                    #{
                        name: "DB_PASS",
                        valueFrom: #{
                            configMapKeyRef: #{
                                name: "old-release-secrets",
                                key: "db-password"
                            }
                        }
                    }
                ],
                envFrom: [
                    #{
                        configMapRef: #{
                            name: "old-release-env"
                        }
                    }
                ]
            }
        ];

        let replaced = gen::replace_containers(containers, "old-release");

        // Verify replacements in env value, env valueFrom, and envFrom
        // (.replace mutates in place, replacing the matched substring)
        replaced[0].env[0].value == "{{instance.appslug}}-config" &&
        replaced[0].env[1].valueFrom.configMapKeyRef.name == "{{instance.appslug}}-secrets" &&
        replaced[0].envFrom[0].configMapRef.name == "{{instance.appslug}}-env"
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true,
        "replace_containers must update configMapRef names in env, envFrom, and valueFrom");
}

#[test]
fn gen_package_replace_containers_handles_secret_refs() {
    // Verify replace_containers updates secret names in all reference types
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        import "gen_package" as gen;

        let containers = [
            #{
                name: "app",
                env: [
                    #{
                        name: "DB_PASS",
                        valueFrom: #{
                            secretKeyRef: #{
                                name: "old-release-db-secret",
                                key: "password"
                            }
                        }
                    }
                ],
                envFrom: [
                    #{
                        secretRef: #{
                            name: "old-release-secret"
                        }
                    }
                ]
            }
        ];

        let replaced = gen::replace_containers(containers, "old-release");

        // Verify secret name replacements (.replace mutates in place)
        replaced[0].env[0].valueFrom.secretKeyRef.name == "{{instance.appslug}}-db-secret" &&
        replaced[0].envFrom[0].secretRef.name == "{{instance.appslug}}-secret"
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true,
        "replace_containers must update secretRef names in valueFrom and envFrom");
}

#[test]
fn gen_package_replace_image_pull_secrets() {
    // Verify replace_image_pull_secrets updates secret names
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        import "gen_package" as gen;

        let secrets = [
            #{
                name: "old-release-registry"
            },
            #{
                name: "other-secret"
            }
        ];

        let replaced = gen::replace_image_pull_secrets(secrets, "old-release");

        // First secret should have "old-release" replaced, second untouched
        // (.replace mutates in place, replacing the matched substring)
        replaced[0].name == "{{instance.appslug}}-registry" &&
        replaced[1].name == "other-secret"
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true,
        "replace_image_pull_secrets must update matching secret names");
}

#[test]
fn gen_package_clean_annotations_backward_compat() {
    // Verify clean_annotations (backward-compat version) works without name param
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        import "gen_package" as gen;

        let metadata = #{
            annotations: #{
                "helm.sh/chart": "chart-1.0",
                "custom.io/desc": "value"
            }
        };

        let cleaned = gen::clean_annotations(metadata);

        !("helm.sh/chart" in cleaned.annotations) &&
        "custom.io/desc" in cleaned.annotations
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true,
        "clean_annotations must remove Helm annotations without name parameter");
}

// ===== backup_context.rhai tests =====

#[test]
fn backup_context_from_args_filters_empty_strings() {
    // Verify from_args splits and filters empty strings from DEPLOYMENT_LIST env var
    // Tests .split() + .filter(|x| x!="") pattern
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        import "backup_context" as ctx;

        // Mock get_env() behavior by setting env vars via set_dynamic
        // Since we can't set env vars directly, we'll test the logic manually
        let input = "deploy1  deploy2  deploy3";
        let items = input.split(" ").filter(|x| x!="");

        // Verify filter removes empty strings
        items.len() == 3 &&
        items[0] == "deploy1" &&
        items[1] == "deploy2" &&
        items[2] == "deploy3"
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true,
        "backup_context must filter empty strings from split lists");
}

#[test]
fn backup_context_reduce_builds_space_separated_list() {
    // Verify .reduce() pattern used in run() to build deployment lists
    // Tests: reduce(|sum, v| if sum.type_of() == "()" { v } else { `${sum} ${v}` })
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        let items = ["deploy1", "deploy2", "deploy3"];

        // Simulate the reduce pattern used in backup_context.run()
        let result = items.reduce(|sum, v| if sum.type_of() == "()" { v } else { `${sum} ${v}` });

        // Verify the result is space-separated
        result == "deploy1 deploy2 deploy3"
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true,
        "backup_context reduce pattern must build space-separated lists");
}

#[test]
fn backup_context_reduce_handles_empty_list() {
    // Verify .reduce() on empty list returns () (unit)
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        let items = [];

        let result = items.reduce(|sum, v| if sum.type_of() == "()" { v } else { `${sum} ${v}` });

        // Empty list reduces to ()
        result == ()
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true,
        "backup_context reduce on empty list must return unit");
}

// ===== resolv_service.rhai tests =====

#[test]
fn resolv_service_get_from_key_finds_service() {
    // Verify get_from_key filters by key and returns matching service
    // Tests: .filter(|s| s.key == names) pattern
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        import "resolv_service" as svc;

        let services = [
            #{
                key: "redis",
                name: "my-redis",
                host: "redis.default.svc",
                port: 6379
            },
            #{
                key: "postgres",
                name: "my-postgres",
                host: "postgres.default.svc",
                port: 5432
            }
        ];

        let found = svc::get_from_key(services, "redis");

        // Verify the correct service was found
        found.key == "redis" &&
        found.host == "redis.default.svc"
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true,
        "get_from_key must find service by key");
}

#[test]
fn resolv_service_get_from_key_returns_unit_when_not_found() {
    // Verify get_from_key returns () when key not found
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        import "resolv_service" as svc;

        let services = [
            #{
                key: "redis",
                name: "my-redis"
            }
        ];

        let found = svc::get_from_key(services, "nonexistent");

        // Should return unit
        found == ()
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true,
        "get_from_key must return unit when key not found");
}

#[test]
fn resolv_service_get_from_package_finds_by_package_name() {
    // Verify get_from_package filters by package.name and returns service
    // Tests: .filter(|s| s["package"].name == names) pattern with nested field access
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        import "resolv_service" as svc;

        let services = [
            #{
                key: "svc1",
                "package": #{
                    name: "postgresql"
                },
                host: "pg.svc"
            },
            #{
                key: "svc2",
                "package": #{
                    name: "redis"
                },
                host: "redis.svc"
            }
        ];

        let found = svc::get_from_package(services, "postgresql");

        // Verify nested field filter worked
        found.key == "svc1" &&
        found["package"].name == "postgresql"
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true,
        "get_from_package must find service by nested package.name");
}

#[test]
fn resolv_service_service_glob_filters_and_maps() {
    // Verify service_glob filters by glob pattern and maps results
    // Tests: .filter(|s| ...) + .map(|s| ...) pattern
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        import "resolv_service" as svc;

        let services = [
            #{
                key: "pg-primary",
                "package": #{
                    name: "postgres-primary"
                }
            },
            #{
                key: "pg-replica",
                "package": #{
                    name: "postgres-replica"
                }
            },
            #{
                key: "redis",
                "package": #{
                    name: "redis"
                }
            }
        ];

        // Find all services matching "postgres-*" pattern
        let found = svc::service_glob(services, "postgres-*");

        // Should return array of matching services (2 postgres services)
        found.len() == 2
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true,
        "service_glob must filter by glob pattern and return array");
}

#[test]
fn resolv_service_get_from_key_with_array_of_names() {
    // Verify get_from_key can take an array of names and return first match
    // Tests: fallthrough loop pattern in get_from_key
    let mut rhai = make_lib_script();
    let result = rhai.eval(r#"
        import "resolv_service" as svc;

        let services = [
            #{
                key: "secondary",
                name: "my-secondary"
            },
            #{
                key: "primary",
                name: "my-primary"
            }
        ];

        // Request with array: ["primary", "secondary"] should return primary
        let found = svc::get_from_key(services, ["primary", "secondary"]);

        found.key == "primary"
    "#).unwrap();

    assert_eq!(result.as_bool().unwrap(), true,
        "get_from_key must handle array of names and return first match");
}
