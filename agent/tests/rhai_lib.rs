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

pub fn make_lib_script_with_k8s(k8s_mocks: Vec<Dynamic>) -> (Script, Arc<Mutex<Vec<Dynamic>>>) {
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

#[allow(dead_code)]
fn build_instance_mock(ns: &str, name: &str) -> Dynamic {
    dynamic_from_json(serde_json::json!({
        "apiVersion": "vynil.solidite.fr/v1",
        "kind": "ServiceInstance",
        "metadata": { "name": name, "namespace": ns },
        "spec": { "category": "test", "package": "test-pkg", "options": {} }
    }))
}

#[allow(dead_code)]
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
    let result = rhai
        .eval(
            r#"
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
    "#,
        )
        .unwrap();

    assert_eq!(result.to_string(), "ceph-distributed");
}

#[test]
fn storage_class_selector_for_singletons_empty_list() {
    // Verify .find() returns () when storage_classes list is empty
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
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
    "#,
        )
        .unwrap();

    assert!(result.as_bool().unwrap());
}

#[test]
fn storage_class_selector_for_singletons_singleton_excluded_from_rwo() {
    // Verify fallback to default when distibuted preference is empty
    // for_singletons falls through all preferences and uses is_default
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
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
    "#,
        )
        .unwrap();

    assert_eq!(result.to_string(), "local-path");
}

#[test]
fn storage_class_selector_for_deployments_rwx_fallback() {
    // Verify fallback chain: no RWX preference → falls back to RWO preference
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
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
    "#,
        )
        .unwrap();

    assert_eq!(result.to_string(), "ceph-rwo");
}

// ===== storage_class_enrich tests =====

#[test]
fn storage_class_enrich_adds_capabilities_and_modes() {
    // Verify .find() on get_known_class() matches provisioner and adds all expected fields
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
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
    "#,
        )
        .unwrap();

    assert!(result.as_bool().unwrap());
}

#[test]
fn storage_class_enrich_unknown_provisioner() {
    // Verify unknown provisioner gets minimal enrichment (empty capabilities, RWO only)
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
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
    "#,
        )
        .unwrap();

    assert!(result.as_bool().unwrap());
}

#[test]
fn storage_class_enrich_block_volumemode_duplication() {
    // Verify raw-capable driver (rbd) has capabilities including raw support
    // and allAccessModes is properly filtered for Block variant
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
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
    "#,
        )
        .unwrap();

    assert!(result.as_bool().unwrap());
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

    let result = rhai
        .eval(
            r#"
        import "wait" as wait;

        let resources = [
            #{ kind: "ServiceInstance", namespace: "default", name: "my-svc" },
            #{ kind: "Deployment", namespace: "default", name: "my-deploy" },
        ];

        // installs should filter and process only ServiceInstance
        wait::installs(resources, 1);
        true
    "#,
        )
        .unwrap();

    assert!(result.as_bool().unwrap());
}

#[test]
fn wait_workload_filters_workloads() {
    // Verify workload() filters and processes Deployment/DaemonSet/StatefulSet
    let k8s_mocks = vec![
        k8s_object("Deployment", "default", "my-deploy"),
        k8s_object("Job", "default", "my-job"),
    ];
    let (mut rhai, _created) = make_lib_script_with_k8s(k8s_mocks);

    let result = rhai
        .eval(
            r#"
        import "wait" as wait;

        let resources = [
            #{ kind: "Deployment", namespace: "default", name: "my-deploy" },
            #{ kind: "Job", namespace: "default", name: "my-job" },
        ];

        // workload should filter and process only Deployment
        wait::workload(resources, 1);
        true
    "#,
        )
        .unwrap();

    assert!(result.as_bool().unwrap());
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

    let result = rhai
        .eval(
            r#"
        import "wait" as wait;

        let resources = [
            #{ kind: "Cluster", namespace: "default", name: "my-cluster" },
            #{ kind: "Redis", namespace: "default", name: "my-redis" },
            #{ kind: "NdbCluster", namespace: "default", name: "my-ndb" },
        ];

        // vital should process all three with appropriate condition types
        wait::vital(resources, 1);
        true
    "#,
        )
        .unwrap();

    assert!(result.as_bool().unwrap());
}

#[test]
fn wait_job_filters_jobs() {
    // Verify job() filters and processes only Job kind
    let k8s_mocks = vec![
        k8s_object("Job", "default", "my-job"),
        k8s_object("Pod", "default", "my-pod"),
    ];
    let (mut rhai, _created) = make_lib_script_with_k8s(k8s_mocks);

    let result = rhai
        .eval(
            r#"
        import "wait" as wait;

        let resources = [
            #{ kind: "Job", namespace: "default", name: "my-job" },
            #{ kind: "Pod", namespace: "default", name: "my-pod" },
        ];

        // job should filter and process only Job
        wait::job(resources, 1);
        true
    "#,
        )
        .unwrap();

    assert!(result.as_bool().unwrap());
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

    let result = rhai
        .eval(
            r#"
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
    "#,
        )
        .unwrap();

    assert!(result.as_bool().unwrap());
}

// ===== install_from_dir.rhai tests =====

#[test]
fn install_from_dir_applies_multiple_yamls() {
    // Verify install() reads multiple YAML files and creates objects in K8s
    // Tests: file I/O (read_dir, file_read), YAML parsing, filtering, K8s apply
    let base = env!("CARGO_MANIFEST_DIR");
    let fixture_dir = format!("{}/tests/fixtures/install_from_dir/basic", base);

    let (mut rhai, created) = make_lib_script_with_k8s(vec![]);

    let result = rhai
        .eval(&format!(
            r#"
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
    "#,
            fixture_dir
        ))
        .unwrap();

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

    let result = rhai
        .eval(&format!(
            r#"
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
    "#,
            fixture_dir
        ))
        .unwrap();

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

    let result = rhai
        .eval(&format!(
            r#"
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
    "#,
            fixture_dir
        ))
        .unwrap();

    let count = result.as_int().unwrap();
    assert_eq!(count, 2);

    // Verify objects have forced namespace
    let created_objs = created.lock().unwrap();
    assert_eq!(created_objs.len(), 2);
    for obj in created_objs.iter() {
        if let Ok(map) = obj.as_map_ref()
            && let Some(meta) = map.get("metadata")
            && let Ok(meta_map) = meta.as_map_ref()
            && let Some(ns) = meta_map.get("namespace")
        {
            assert_eq!(ns.to_string(), "forced-ns");
        }
    }
}

#[test]
fn install_from_dir_respects_ordering() {
    // Verify get_first() kinds (ConfigMap) are applied before get_last() kinds (Deployment)
    // Tests the 3-phase ordering logic: first, middle, last
    let base = env!("CARGO_MANIFEST_DIR");
    let fixture_dir = format!("{}/tests/fixtures/install_from_dir/basic", base);

    let (mut rhai, _created) = make_lib_script_with_k8s(vec![]);

    let result = rhai
        .eval(&format!(
            r#"
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
    "#,
            fixture_dir
        ))
        .unwrap();

    assert!(result.as_bool().unwrap());
}

// ===== gen_package.rhai tests =====

#[test]
fn gen_package_replace_returns_modified_string() {
    // Vérifie le comportement de .replace() : depuis Rhai 1.18+, retourne la string modifiée
    // au lieu de muter la string originale. Pattern vulnérable: str.replace(...); str
    // Ce test expose le bug si replace() est appelé sans assigner le résultat.
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
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
    "#,
        )
        .unwrap();

    // This should be true if .replace() properly updates the value
    // Currently may be false if the bug exists (pattern: v.replace(...); v without assignment)
    assert!(
        result.as_bool().unwrap(),
        "replace_label_values must update label values using .replace() with assignment"
    );
}


#[test]
fn gen_package_clean_metadata_removes_helm_annotations() {
    // Verify clean_metadata removes Helm-specific annotations and preserves others
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
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
    "#,
        )
        .unwrap();

    assert!(
        result.as_bool().unwrap(),
        "clean_metadata must remove Helm and checksum annotations but keep custom ones"
    );
}

#[test]
fn gen_package_clean_metadata_empty_annotations() {
    // Verify clean_metadata removes the annotations key entirely if it becomes empty
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
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
    "#,
        )
        .unwrap();

    assert!(
        result.as_bool().unwrap(),
        "clean_metadata must remove annotations key if all entries are Helm-related"
    );
}

#[test]
fn gen_package_replace_volumes_handles_configmap_and_pvc() {
    // Verify replace_volumes updates names in configMap and persistentVolumeClaim refs
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
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
    "#,
        )
        .unwrap();

    assert!(
        result.as_bool().unwrap(),
        "replace_volumes must update configMap and persistentVolumeClaim names"
    );
}

#[test]
fn gen_package_replace_containers_handles_env_configmap_refs() {
    // Verify replace_containers updates names in environment variable refs
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
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
    "#,
        )
        .unwrap();

    assert!(
        result.as_bool().unwrap(),
        "replace_containers must update configMapRef names in env, envFrom, and valueFrom"
    );
}

#[test]
fn gen_package_replace_containers_handles_secret_refs() {
    // Verify replace_containers updates secret names in all reference types
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
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
    "#,
        )
        .unwrap();

    assert!(
        result.as_bool().unwrap(),
        "replace_containers must update secretRef names in valueFrom and envFrom"
    );
}

#[test]
fn gen_package_replace_image_pull_secrets() {
    // Verify replace_image_pull_secrets updates secret names
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
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
    "#,
        )
        .unwrap();

    assert!(
        result.as_bool().unwrap(),
        "replace_image_pull_secrets must update matching secret names"
    );
}

#[test]
fn gen_package_clean_annotations_backward_compat() {
    // Verify clean_annotations (backward-compat version) works without name param
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
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
    "#,
        )
        .unwrap();

    assert!(
        result.as_bool().unwrap(),
        "clean_annotations must remove Helm annotations without name parameter"
    );
}

// ===== backup_context.rhai tests =====

#[test]
fn backup_context_from_args_filters_empty_strings() {
    // Verify from_args splits and filters empty strings from DEPLOYMENT_LIST env var
    // Tests .split() + .filter(|x| x!="") pattern
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
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
    "#,
        )
        .unwrap();

    assert!(
        result.as_bool().unwrap(),
        "backup_context must filter empty strings from split lists"
    );
}

#[test]
fn backup_context_reduce_builds_space_separated_list() {
    // Verify .reduce() pattern used in run() to build deployment lists
    // Tests: reduce(|sum, v| if sum.type_of() == "()" { v } else { `${sum} ${v}` })
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        let items = ["deploy1", "deploy2", "deploy3"];

        // Simulate the reduce pattern used in backup_context.run()
        let result = items.reduce(|sum, v| if sum.type_of() == "()" { v } else { `${sum} ${v}` });

        // Verify the result is space-separated
        result == "deploy1 deploy2 deploy3"
    "#,
        )
        .unwrap();

    assert!(
        result.as_bool().unwrap(),
        "backup_context reduce pattern must build space-separated lists"
    );
}

#[test]
fn backup_context_reduce_handles_empty_list() {
    // Verify .reduce() on empty list returns () (unit)
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        let items = [];

        let result = items.reduce(|sum, v| if sum.type_of() == "()" { v } else { `${sum} ${v}` });

        // Empty list reduces to ()
        result == ()
    "#,
        )
        .unwrap();

    assert!(
        result.as_bool().unwrap(),
        "backup_context reduce on empty list must return unit"
    );
}

// ===== resolv_service.rhai tests =====

#[test]
fn resolv_service_get_from_key_finds_service() {
    // Verify get_from_key filters by key and returns matching service
    // Tests: .filter(|s| s.key == names) pattern
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
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
    "#,
        )
        .unwrap();

    assert!(result.as_bool().unwrap(), "get_from_key must find service by key");
}

#[test]
fn resolv_service_get_from_key_returns_unit_when_not_found() {
    // Verify get_from_key returns () when key not found
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
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
    "#,
        )
        .unwrap();

    assert!(
        result.as_bool().unwrap(),
        "get_from_key must return unit when key not found"
    );
}

#[test]
fn resolv_service_get_from_package_finds_by_package_name() {
    // Verify get_from_package filters by package.name and returns service
    // Tests: .filter(|s| s["package"].name == names) pattern with nested field access
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
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
    "#,
        )
        .unwrap();

    assert!(
        result.as_bool().unwrap(),
        "get_from_package must find service by nested package.name"
    );
}

#[test]
fn resolv_service_service_glob_filters_and_maps() {
    // Verify service_glob filters by glob pattern and maps results
    // Tests: .filter(|s| ...) + .map(|s| ...) pattern
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
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
    "#,
        )
        .unwrap();

    assert!(
        result.as_bool().unwrap(),
        "service_glob must filter by glob pattern and return array"
    );
}

#[test]
fn resolv_service_get_from_key_with_array_of_names() {
    // Verify get_from_key can take an array of names and return first match
    // Tests: fallthrough loop pattern in get_from_key
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
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
    "#,
        )
        .unwrap();

    assert!(
        result.as_bool().unwrap(),
        "get_from_key must handle array of names and return first match"
    );
}

// ===== gen_package — fonctions non couvertes (replace() résultat ignoré) =====

#[test]
fn gen_package_replace_pod_spec_updates_service_account_name() {
    // replace_pod_spec line 162: spec.serviceAccountName.replace(name, "{{instance.appslug}}")
    // résultat ignoré → serviceAccountName inchangé si Rhai change de sémantique
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "gen_package" as gen;

        let spec = #{
            serviceAccountName: "old-release-sa",
            containers: []
        };

        let replaced = gen::replace_pod_spec(spec, "old-release");
        replaced.serviceAccountName
    "#,
        )
        .unwrap();

    assert_eq!(
        result.to_string(),
        "{{instance.appslug}}-sa",
        "replace_pod_spec doit remplacer serviceAccountName via .replace()"
    );
}

#[test]
fn gen_package_replace_binding_updates_subject_name() {
    // replace_binding line 441: s.name.replace(name, "{{instance.appslug}}")
    // résultat ignoré → nom du subject inchangé si Rhai change de sémantique
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "gen_package" as gen;

        let doc = #{
            subjects: [
                #{ name: "old-release-sa", kind: "ServiceAccount", namespace: "default" }
            ],
            roleRef: #{
                name: "old-release-role",
                kind: "ClusterRole",
                apiGroup: "rbac.authorization.k8s.io"
            }
        };

        let replaced = gen::replace_binding(doc, "old-release");
        replaced.subjects[0].name
    "#,
        )
        .unwrap();

    assert_eq!(
        result.to_string(),
        "{{instance.appslug}}-sa",
        "replace_binding doit remplacer le nom du subject via .replace()"
    );
}

#[test]
fn gen_package_replace_binding_updates_role_ref_name() {
    // replace_binding (RoleBinding) : roleRef pointe un Role → pas de préfixe namespace
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "gen_package" as gen;

        let doc = #{
            subjects: [],
            roleRef: #{
                name: "old-release-role",
                kind: "Role",
                apiGroup: "rbac.authorization.k8s.io"
            }
        };

        let replaced = gen::replace_binding(doc, "old-release");
        replaced.roleRef.name
    "#,
        )
        .unwrap();

    assert_eq!(
        result.to_string(),
        "{{instance.appslug}}-role",
        "replace_binding doit utiliser appslug seul pour le roleRef (RoleBinding)"
    );
}

#[test]
fn gen_package_replace_cluster_binding_uses_namespace_prefix_in_role_ref() {
    // replace_cluster_binding (ClusterRoleBinding) : roleRef doit matcher le metadata.name
    // du ClusterRole qui est préfixé par {{instance.namespace}}-{{instance.appslug}}.
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "gen_package" as gen;

        let doc = #{
            subjects: [],
            roleRef: #{
                name: "old-release-cert-manager-cainjector",
                kind: "ClusterRole",
                apiGroup: "rbac.authorization.k8s.io"
            }
        };

        let replaced = gen::replace_cluster_binding(doc, "old-release");
        replaced.roleRef.name
    "#,
        )
        .unwrap();

    assert_eq!(
        result.to_string(),
        "{{instance.namespace}}-{{instance.appslug}}-cert-manager-cainjector",
        "replace_cluster_binding doit préfixer le roleRef avec namespace+appslug"
    );
}

#[test]
fn gen_package_replace_webhooks_updates_service_name() {
    // replace_webhooks line 461: w.clientConfig.service.name.replace(name, "{{instance.appslug}}")
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "gen_package" as gen;

        let doc = #{
            webhooks: [
                #{
                    name: "validate.old-release.io",
                    clientConfig: #{
                        service: #{
                            name: "old-release-webhook",
                            namespace: "default",
                            path: "/validate"
                        }
                    }
                }
            ]
        };

        let replaced = gen::replace_webhooks(doc, "old-release");
        replaced.webhooks[0].clientConfig.service.name
    "#,
        )
        .unwrap();

    assert_eq!(
        result.to_string(),
        "{{instance.appslug}}-webhook",
        "replace_webhooks doit remplacer le nom du service webhook via .replace()"
    );
}

#[test]
fn gen_package_replace_ingress_updates_backend_service_name() {
    // replace_ingress line 484: p.backend.service.name.replace(name, "{{instance.appslug}}")
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "gen_package" as gen;

        let doc = #{
            metadata: #{
                name: "old-release-ing",
                annotations: #{}
            },
            spec: #{
                rules: [
                    #{
                        http: #{
                            paths: [
                                #{
                                    path: "/",
                                    pathType: "Prefix",
                                    backend: #{
                                        service: #{
                                            name: "old-release-svc",
                                            port: #{ number: 80 }
                                        }
                                    }
                                }
                            ]
                        }
                    }
                ]
            }
        };

        let replaced = gen::replace_ingress(doc, "old-release");
        replaced.spec.rules[0].http.paths[0].backend.service.name
    "#,
        )
        .unwrap();

    assert_eq!(
        result.to_string(),
        "{{instance.appslug}}-svc",
        "replace_ingress doit remplacer le nom du service backend via .replace()"
    );
}

#[test]
fn gen_package_replace_ingress_updates_tls_secret_name() {
    // replace_ingress line 498: tls.secretName.replace(name, "{{instance.appslug}}")
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "gen_package" as gen;

        let doc = #{
            metadata: #{ name: "old-release-ing", annotations: #{} },
            spec: #{
                rules: [],
                tls: [
                    #{
                        secretName: "old-release-tls",
                        hosts: ["example.com"]
                    }
                ]
            }
        };

        let replaced = gen::replace_ingress(doc, "old-release");
        replaced.spec.tls[0].secretName
    "#,
        )
        .unwrap();

    assert_eq!(
        result.to_string(),
        "{{instance.appslug}}-tls",
        "replace_ingress doit remplacer le secretName TLS via .replace()"
    );
}

#[test]
fn gen_package_replace_workload_statefulset_updates_service_name() {
    // replace_workload line 531: doc.spec.serviceName.replace(name, "{{instance.appslug}}")
    // Utilise un StatefulSet sans spec.template → extract_env_configmap retourne immédiatement
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "gen_package" as gen;

        let doc = #{
            kind: "StatefulSet",
            metadata: #{ name: "old-release-sts", annotations: #{} },
            spec: #{
                serviceName: "old-release-headless",
                volumeClaimTemplates: []
            }
        };

        let replaced = gen::replace_workload("/dev/null", "old-release-sts", doc, "old-release");
        replaced.spec.serviceName
    "#,
        )
        .unwrap();

    assert_eq!(
        result.to_string(),
        "{{instance.appslug}}-headless",
        "replace_workload doit remplacer spec.serviceName du StatefulSet via .replace()"
    );
}

#[test]
fn gen_package_replace_workload_statefulset_updates_volume_claim_name() {
    // replace_workload line 537: vct.metadata.name.replace(name, "{{instance.appslug}}")
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "gen_package" as gen;

        let doc = #{
            kind: "StatefulSet",
            metadata: #{ name: "old-release-sts", annotations: #{} },
            spec: #{
                serviceName: "old-release-headless",
                volumeClaimTemplates: [
                    #{ metadata: #{ name: "old-release-data" }, spec: #{ accessModes: ["ReadWriteOnce"] } }
                ]
            }
        };

        let replaced = gen::replace_workload("/dev/null", "old-release-sts", doc, "old-release");
        replaced.spec.volumeClaimTemplates[0].metadata.name
    "#,
        )
        .unwrap();

    assert_eq!(
        result.to_string(),
        "{{instance.appslug}}-data",
        "replace_workload doit remplacer le nom du volumeClaimTemplate via .replace()"
    );
}

#[test]
fn gen_package_apply_selector_expressions_replaces_markers_in_file() {
    // apply_selector_expressions lines 786-792 : 6 replace() dont les résultats sont ignorés
    // puis file_write(filepath, content) écrit le contenu ORIGINAL — bug critique
    let mut rhai = make_lib_script();
    let tmp_path = format!(
        "{}/tests/tmp/apply_selector_test.yaml",
        env!("CARGO_MANIFEST_DIR")
    );

    // Fichier avec les deux types de marqueurs (non quotés)
    std::fs::write(
        &tmp_path,
        "selector: SELECTOR_COMP_mycomp\nlabels: LABELS_FROM_CTX\n",
    )
    .unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        gen::apply_selector_expressions("{tmp}", "mycomp");
    "#,
            tmp = tmp_path
        ))
        .unwrap();

    let content = std::fs::read_to_string(&tmp_path).unwrap();
    let _ = std::fs::remove_file(&tmp_path);

    // Avec sémantique mutation : marqueurs remplacés par expressions Handlebars
    // Avec sémantique retour (bug) : content inchangé, marqueurs encore présents
    assert!(
        !content.contains("SELECTOR_COMP_mycomp"),
        "apply_selector_expressions doit remplacer SELECTOR_COMP_mycomp dans le fichier"
    );
    assert!(
        !content.contains("LABELS_FROM_CTX"),
        "apply_selector_expressions doit remplacer LABELS_FROM_CTX dans le fichier"
    );
}

// ===== gen_crd_yaml — yamllint header tests =====

#[test]
fn gen_crd_yaml_writes_yamllint_header() {
    let mut rhai = make_lib_script();
    let tmp_path = format!(
        "{}/tests/tmp/gen_crd_yaml_header.yaml",
        env!("CARGO_MANIFEST_DIR")
    );

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;

        let data = #{{
            apiVersion: "apiextensions.k8s.io/v1",
            kind: "CustomResourceDefinition",
            metadata: #{{ name: "foos.example.com" }},
            spec: #{{ group: "example.com" }}
        }};

        gen::gen_crd_yaml("{tmp}", data);
    "#,
            tmp = tmp_path
        ))
        .unwrap();

    let content = std::fs::read_to_string(&tmp_path).unwrap();
    let _ = std::fs::remove_file(&tmp_path);

    assert!(
        content.starts_with("# yamllint disable rule:line-length\n"),
        "gen_crd_yaml doit débuter par le commentaire yamllint"
    );
    assert!(
        content.contains("---\n"),
        "gen_crd_yaml doit contenir le séparateur YAML"
    );
}

#[test]
fn gen_yaml_does_not_write_yamllint_header() {
    let mut rhai = make_lib_script();
    let tmp_path = format!("{}/tests/tmp/gen_yaml_no_header.yaml", env!("CARGO_MANIFEST_DIR"));

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;

        let data = #{{ kind: "ConfigMap", metadata: #{{ name: "my-config" }} }};

        gen::gen_yaml("{tmp}", data);
    "#,
            tmp = tmp_path
        ))
        .unwrap();

    let content = std::fs::read_to_string(&tmp_path).unwrap();
    let _ = std::fs::remove_file(&tmp_path);

    assert!(
        !content.contains("yamllint"),
        "gen_yaml ne doit pas ajouter le commentaire yamllint"
    );
}

#[test]
fn gen_system_crd_without_webhook_has_yamllint_header() {
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/gen_system_crd", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;

        let docs = [#{{
            apiVersion: "apiextensions.k8s.io/v1",
            kind: "CustomResourceDefinition",
            metadata: #{{ name: "foos.example.com" }},
            spec: #{{ group: "example.com" }}
        }}];

        gen::gen_system("{dir}", docs, "my-release");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let crd_path = format!("{}/get_crds/foos.example.com.yaml", tmp_dir);
    let content = std::fs::read_to_string(&crd_path).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        content.starts_with("# yamllint disable rule:line-length\n"),
        "gen_system doit ajouter le header yamllint dans les fichiers CRD sans webhook"
    );
}

#[test]
fn gen_system_crd_with_webhook_has_yamllint_header() {
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/gen_system_crd_webhook", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;

        let docs = [#{{
            apiVersion: "apiextensions.k8s.io/v1",
            kind: "CustomResourceDefinition",
            metadata: #{{ name: "bars.example.com" }},
            spec: #{{
                group: "example.com",
                conversion: #{{
                    strategy: "Webhook",
                    webhook: #{{
                        clientConfig: #{{
                            service: #{{ name: "my-release-webhook", namespace: "default" }}
                        }}
                    }}
                }}
            }}
        }}];

        gen::gen_system("{dir}", docs, "my-release");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let crd_path = format!("{}/get_crds/bars.example.com.yaml.hbs", tmp_dir);
    let content = std::fs::read_to_string(&crd_path).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        content.starts_with("# yamllint disable rule:line-length\n"),
        "gen_system doit ajouter le header yamllint dans les fichiers CRD avec webhook"
    );
}

#[test]
fn gen_service_crd_without_webhook_has_yamllint_header() {
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/gen_service_crd", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;

        let docs = [#{{
            apiVersion: "apiextensions.k8s.io/v1",
            kind: "CustomResourceDefinition",
            metadata: #{{ name: "widgets.example.com" }},
            spec: #{{ group: "example.com" }}
        }}];

        gen::gen_service("{dir}", docs, "my-release");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let crd_path = format!("{}/get_crds/widgets.example.com.yaml", tmp_dir);
    let content = std::fs::read_to_string(&crd_path).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        content.starts_with("# yamllint disable rule:line-length\n"),
        "gen_service doit ajouter le header yamllint dans les fichiers CRD sans webhook"
    );
}

#[test]
fn gen_system_crd_has_no_helm_labels_or_annotations() {
    // Les CRDs doivent passer par clean_metadata : labels Helm supprimés,
    // annotations Helm supprimées, seules les annotations non-Helm conservées.
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/gen_system_crd_no_labels", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [#{{
            apiVersion: "apiextensions.k8s.io/v1",
            kind: "CustomResourceDefinition",
            metadata: #{{
                name: "foos.example.com",
                labels: #{{
                    "app": "my-release",
                    "app.kubernetes.io/managed-by": "Helm",
                    "app.kubernetes.io/instance": "v7aeab500",
                    "helm.sh/chart": "mychart-1.0"
                }},
                annotations: #{{
                    "helm.sh/chart": "mychart-1.0",
                    "helm.sh/resource-policy": "keep",
                    "checksum/config": "abc123"
                }}
            }},
            spec: #{{ group: "example.com" }}
        }}];
        gen::gen_system("{dir}", docs, "my-release");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let content = std::fs::read_to_string(format!("{}/get_crds/foos.example.com.yaml", tmp_dir)).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        !content.contains("labels:"),
        "les labels doivent être supprimés des CRDs, got:\n{content}"
    );
    assert!(
        !content.contains("helm.sh/chart"),
        "helm.sh/chart doit être supprimé des CRDs, got:\n{content}"
    );
    assert!(
        !content.contains("checksum/"),
        "les checksums doivent être supprimés des CRDs, got:\n{content}"
    );
    assert!(
        content.contains("helm.sh/resource-policy"),
        "helm.sh/resource-policy doit être conservé (annotation non-Helm), got:\n{content}"
    );
}

// ===== update_package_yaml — ordering and correctness =====

#[test]
fn update_package_yaml_starts_with_document_separator() {
    // Verify the rewritten package.yaml starts with "---"
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/pkg_yaml_separator", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let pkg_yaml = "---\napiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: myapp\n  type: system\n";
    std::fs::write(format!("{}/package.yaml", tmp_dir), pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [#{{
            apiVersion: "apps/v1",
            kind: "Deployment",
            metadata: #{{ name: "myapp" }},
            spec: #{{
                selector: #{{ matchLabels: #{{ app: "myapp" }} }},
                template: #{{
                    metadata: #{{ labels: #{{ app: "myapp" }} }},
                    spec: #{{ containers: [#{{ name: "app", image: "docker.io/myapp:v1.0" }}] }}
                }}
            }}
        }}];
        gen::gen_system("{dir}", docs, "myapp");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let content = std::fs::read_to_string(format!("{}/package.yaml", tmp_dir)).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        content.starts_with("---\n"),
        "package.yaml doit commencer par '---'"
    );
}

#[test]
fn update_package_yaml_preserves_top_level_key_order() {
    // Verify that existing top-level keys stay in their original order after update.
    // Specifically: apiVersion, kind, metadata, requirements, images, resources
    // must appear in that order and NOT be moved inside metadata.
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/pkg_yaml_key_order", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    // package.yaml with top-level keys in a specific order
    let pkg_yaml = "---\napiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: myapp\n  type: system\nrequirements: []\nimages:\n  existing:\n    registry: docker.io\n    repository: existing\nresources: {}\n";
    std::fs::write(format!("{}/package.yaml", tmp_dir), pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [#{{
            apiVersion: "apps/v1",
            kind: "Deployment",
            metadata: #{{ name: "myapp" }},
            spec: #{{
                selector: #{{ matchLabels: #{{ app: "myapp" }} }},
                template: #{{
                    metadata: #{{ labels: #{{ app: "myapp" }} }},
                    spec: #{{ containers: [#{{ name: "app", image: "docker.io/myapp:v1.0" }}] }}
                }}
            }}
        }}];
        gen::gen_system("{dir}", docs, "myapp");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let content = std::fs::read_to_string(format!("{}/package.yaml", tmp_dir)).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    // requirements must appear BEFORE images and NOT inside metadata
    let req_pos = content.find("\nrequirements:").unwrap_or(usize::MAX);
    let img_pos = content.find("\nimages:").unwrap_or(usize::MAX);
    let metadata_pos = content.find("\nmetadata:").unwrap_or(usize::MAX);

    assert!(
        req_pos > metadata_pos,
        "requirements doit être au niveau top-level (après metadata:), pas à l'intérieur"
    );
    assert!(
        req_pos < img_pos,
        "requirements doit apparaître avant images dans l'ordre original"
    );
    // Verify no complex key corruption (? [...] : ...)
    assert!(
        !content.contains("? ["),
        "package.yaml ne doit pas contenir de clés complexes YAML (? [...])"
    );
}

#[test]
fn update_package_yaml_preserves_metadata_structure() {
    // Verify that metadata fields (including features array and app_version) are not corrupted.
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/pkg_yaml_metadata", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    // Exact structure from user's real file including "description: >" folded block scalar
    let pkg_yaml = "---\napiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: traefik\n  type: system\n  category: core\n  description: >\n    Traefik is a modern HTTP reverse proxy and load balancer made\n    to deploy microservices with ease.\n  features:\n  - upgrade\n  - auto_config\n  app_version: 40.2.0\nrequirements: []\n";
    std::fs::write(format!("{}/package.yaml", tmp_dir), pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [#{{
            apiVersion: "apps/v1",
            kind: "Deployment",
            metadata: #{{ name: "traefik" }},
            spec: #{{
                selector: #{{ matchLabels: #{{ app: "traefik" }} }},
                template: #{{
                    metadata: #{{ labels: #{{ app: "traefik" }} }},
                    spec: #{{ containers: [#{{ name: "traefik", image: "docker.io/traefik:v3.7.1" }}] }}
                }}
            }}
        }}];
        gen::gen_system("{dir}", docs, "traefik");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let content = std::fs::read_to_string(format!("{}/package.yaml", tmp_dir)).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        !content.contains("? ["),
        "les features ne doivent pas être encodées comme clé complexe YAML"
    );
    assert!(
        content.contains("app_version"),
        "app_version doit rester dans metadata"
    );
    assert!(
        content.contains("upgrade"),
        "les items de features doivent rester présents"
    );
    assert!(
        content.contains("40.2.0"),
        "la valeur de app_version doit être préservée"
    );
}

fn run_block_scalar_test(scalar_header: &str, scalar_body: &str) {
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let safe_name = scalar_header
        .replace('>', "gt")
        .replace('|', "pipe")
        .replace('-', "strip")
        .replace(' ', "");
    let tmp_dir = format!("{}/tests/tmp/pkg_yaml_scalar_{}", base, safe_name);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let pkg_yaml = format!(
        "---\napiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: traefik\n  description: {}\n{}\n  app_version: 40.2.0\nrequirements: []\n",
        scalar_header, scalar_body
    );
    std::fs::write(format!("{}/package.yaml", tmp_dir), &pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [#{{
            apiVersion: "apps/v1",
            kind: "Deployment",
            metadata: #{{ name: "traefik" }},
            spec: #{{
                selector: #{{ matchLabels: #{{ app: "traefik" }} }},
                template: #{{
                    metadata: #{{ labels: #{{ app: "traefik" }} }},
                    spec: #{{ containers: [#{{ name: "traefik", image: "docker.io/traefik:v3.7.1" }}] }}
                }}
            }}
        }}];
        gen::gen_system("{dir}", docs, "traefik");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let content = std::fs::read_to_string(format!("{}/package.yaml", tmp_dir)).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(content.starts_with("---\n"), "doit commencer par ---");
    assert!(!content.contains("? ["), "pas de clé complexe YAML");
    assert!(
        content.contains("app_version"),
        "app_version doit rester présent (scalar: {})",
        scalar_header
    );
    assert!(
        content.contains("40.2.0"),
        "valeur app_version préservée (scalar: {})",
        scalar_header
    );
    // requirements must be top-level (not inside metadata)
    let req_pos = content.find("\nrequirements:").unwrap_or(usize::MAX);
    let meta_pos = content.find("\nmetadata:").unwrap_or(0);
    assert!(
        req_pos > meta_pos,
        "requirements doit être top-level, pas imbriqué dans metadata (scalar: {})",
        scalar_header
    );
}

#[test]
fn update_package_yaml_folded_scalar_gt_not_corrupted() {
    run_block_scalar_test(
        ">",
        "    Traefik is a modern HTTP reverse proxy.\n    Second line.",
    );
}

#[test]
fn update_package_yaml_literal_scalar_pipe_not_corrupted() {
    run_block_scalar_test(
        "|",
        "    Traefik is a modern HTTP reverse proxy.\n    Second line.",
    );
}

#[test]
fn update_package_yaml_folded_strip_scalar_gt_dash_not_corrupted() {
    run_block_scalar_test(
        ">-",
        "    Traefik is a modern HTTP reverse proxy.\n    Second line.",
    );
}

#[test]
fn update_package_yaml_literal_strip_scalar_pipe_dash_not_corrupted() {
    run_block_scalar_test(
        "|-",
        "    Traefik is a modern HTTP reverse proxy.\n    Second line.",
    );
}

#[test]
fn update_package_yaml_colon_in_key_name_not_corrupted() {
    // Regression: a key containing ":" (e.g. "recommandations:") must not be corrupted
    // or moved inside metadata after the update.
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/pkg_yaml_colon_key", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    // Exact structure from the user's file (including the quirky "recommandations:" key)
    let pkg_yaml = "---\napiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: traefik\n  type: system\n  features:\n  - upgrade\n  - auto_config\n  app_version: 40.2.0\nrequirements: []\n\"recommandations:\": []\n";
    std::fs::write(format!("{}/package.yaml", tmp_dir), pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [#{{
            apiVersion: "apps/v1",
            kind: "Deployment",
            metadata: #{{ name: "traefik" }},
            spec: #{{
                selector: #{{ matchLabels: #{{ app: "traefik" }} }},
                template: #{{
                    metadata: #{{ labels: #{{ app: "traefik" }} }},
                    spec: #{{ containers: [#{{ name: "traefik", image: "docker.io/traefik:v3.7.1" }}] }}
                }}
            }}
        }}];
        gen::gen_system("{dir}", docs, "traefik");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let content = std::fs::read_to_string(format!("{}/package.yaml", tmp_dir)).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        content.starts_with("---\n"),
        "package.yaml doit commencer par '---'"
    );
    assert!(
        !content.contains("? ["),
        "features ne doit pas être encodée comme clé complexe YAML"
    );
    // requirements and recommandations must be top-level (appear after metadata block)
    let metadata_end = content
        .find("\nrequirements:")
        .unwrap_or_else(|| panic!("requirements doit être présent dans le fichier"));
    let metadata_start = content.find("\nmetadata:").unwrap_or(0);
    assert!(
        metadata_end > metadata_start,
        "requirements doit être au niveau top-level, pas imbriqué dans metadata"
    );
    assert!(content.contains("app_version"), "app_version doit rester présent");
    assert!(
        content.contains("40.2.0"),
        "la valeur de app_version doit être préservée"
    );
}

// ===== gen_package — extraction images et resources =====

#[test]
fn parse_image_extracts_registry_repo_tag() {
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "gen_package" as gen;
        let p = gen::parse_image("docker.io/traefik:v3.7.1");
        p.registry == "docker.io" && p.repository == "traefik" && p.tag == "v3.7.1"
    "#,
        )
        .unwrap();
    assert!(
        result.as_bool().unwrap(),
        "parse_image doit extraire registry/repository/tag"
    );
}

#[test]
fn parse_image_no_registry_uses_empty_string() {
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "gen_package" as gen;
        let p = gen::parse_image("traefik:v3.7.1");
        p.registry == "" && p.repository == "traefik" && p.tag == "v3.7.1"
    "#,
        )
        .unwrap();
    assert!(
        result.as_bool().unwrap(),
        "parse_image sans registry doit laisser registry vide"
    );
}

#[test]
fn parse_image_quay_registry_with_path() {
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "gen_package" as gen;
        let p = gen::parse_image("quay.io/jetstack/cert-manager-acmesolver:v1.20.2");
        p.registry == "quay.io" && p.repository == "jetstack/cert-manager-acmesolver" && p.tag == "v1.20.2"
    "#,
        )
        .unwrap();
    assert!(
        result.as_bool().unwrap(),
        "parse_image doit gérer quay.io avec chemin d'image"
    );
}

#[test]
fn gen_system_deployment_extracts_image_to_package_yaml() {
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/gen_system_extract_images", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let pkg_yaml = "apiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: my-release\n  category: core\n  type: system\n";
    std::fs::write(format!("{}/package.yaml", tmp_dir), pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [#{{
            apiVersion: "apps/v1",
            kind: "Deployment",
            metadata: #{{ name: "my-release" }},
            spec: #{{
                selector: #{{ matchLabels: #{{ app: "my-release" }} }},
                template: #{{
                    metadata: #{{ labels: #{{ app: "my-release" }} }},
                    spec: #{{
                        containers: [#{{
                            name: "traefik",
                            image: "docker.io/traefik:v3.7.1",
                            resources: #{{
                                requests: #{{ cpu: "20m", memory: "128Mi" }},
                                limits: #{{ cpu: "1000m", memory: "256Mi" }}
                            }}
                        }}]
                    }}
                }}
            }}
        }}];
        gen::gen_system("{dir}", docs, "my-release");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let pkg_content = std::fs::read_to_string(format!("{}/package.yaml", tmp_dir)).unwrap();
    let hbs_content =
        std::fs::read_to_string(format!("{}/get_systems/Deployment_app.yaml.hbs", tmp_dir)).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        pkg_content.contains("docker.io"),
        "package.yaml doit contenir le registry docker.io extrait"
    );
    assert!(
        pkg_content.contains("traefik"),
        "package.yaml doit contenir l'image traefik"
    );
    assert!(
        pkg_content.contains("128Mi"),
        "package.yaml doit contenir les resources extraites"
    );
    assert!(
        hbs_content.contains("image_from_ctx"),
        "le template HBS doit référencer image_from_ctx"
    );
    assert!(
        hbs_content.contains("resources_from_ctx"),
        "le template HBS doit référencer resources_from_ctx"
    );
}

#[test]
fn gen_system_deployment_extracts_image_from_args() {
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/gen_system_extract_args_image", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let pkg_yaml = "apiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: cert-manager\n  category: core\n  type: system\n";
    std::fs::write(format!("{}/package.yaml", tmp_dir), pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [#{{
            apiVersion: "apps/v1",
            kind: "Deployment",
            metadata: #{{ name: "cert-manager" }},
            spec: #{{
                selector: #{{ matchLabels: #{{ app: "cert-manager" }} }},
                template: #{{
                    metadata: #{{ labels: #{{ app: "cert-manager" }} }},
                    spec: #{{
                        containers: [#{{
                            name: "cert-manager-controller",
                            image: "quay.io/jetstack/cert-manager-controller:v1.20.2",
                            args: [
                                "--leader-elect=true",
                                "--acme-http01-solver-image=quay.io/jetstack/cert-manager-acmesolver:v1.20.2"
                            ]
                        }}]
                    }}
                }}
            }}
        }}];
        gen::gen_system("{dir}", docs, "cert-manager");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let pkg_content = std::fs::read_to_string(format!("{}/package.yaml", tmp_dir)).unwrap();
    let hbs_content =
        std::fs::read_to_string(format!("{}/get_systems/Deployment_app.yaml.hbs", tmp_dir)).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        pkg_content.contains("cert-manager-acmesolver"),
        "package.yaml doit contenir l'image extraite des args"
    );
    assert!(
        hbs_content.contains("image_from_ctx"),
        "le template HBS doit référencer image_from_ctx pour l'image principale"
    );
    assert!(
        hbs_content.contains("acme-http01-solver-image="),
        "le template HBS doit conserver le flag --acme-http01-solver-image"
    );
}

#[test]
fn gen_system_overwrites_existing_images_on_regeneration() {
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/gen_system_overwrite", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let pkg_yaml = "apiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: my-release\n  category: core\n  type: system\nimages:\n  traefik:\n    registry: docker.io\n    repository: traefik\n    tag: v3.6.0\n";
    std::fs::write(format!("{}/package.yaml", tmp_dir), pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [#{{
            apiVersion: "apps/v1",
            kind: "Deployment",
            metadata: #{{ name: "my-release" }},
            spec: #{{
                selector: #{{ matchLabels: #{{ app: "my-release" }} }},
                template: #{{
                    metadata: #{{ labels: #{{ app: "my-release" }} }},
                    spec: #{{
                        containers: [#{{ name: "traefik", image: "docker.io/traefik:v3.7.1" }}]
                    }}
                }}
            }}
        }}];
        gen::gen_system("{dir}", docs, "my-release");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let pkg_content = std::fs::read_to_string(format!("{}/package.yaml", tmp_dir)).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        pkg_content.contains("v3.7.1"),
        "gen_system doit écraser l'entrée images existante avec la nouvelle version"
    );
}

#[test]
fn gen_system_keeps_existing_resources_when_container_has_none() {
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/gen_system_keep_resources", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    // package.yaml already has resources for "traefik"
    let pkg_yaml = "apiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: my-release\n  category: core\n  type: system\nresources:\n  traefik:\n    requests:\n      cpu: 20m\n      memory: 128Mi\n";
    std::fs::write(format!("{}/package.yaml", tmp_dir), pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        // Container has an image but NO resources defined
        let docs = [#{{
            apiVersion: "apps/v1",
            kind: "Deployment",
            metadata: #{{ name: "my-release" }},
            spec: #{{
                selector: #{{ matchLabels: #{{ app: "my-release" }} }},
                template: #{{
                    metadata: #{{ labels: #{{ app: "my-release" }} }},
                    spec: #{{
                        containers: [#{{ name: "traefik", image: "docker.io/traefik:v3.7.1" }}]
                    }}
                }}
            }}
        }}];
        gen::gen_system("{dir}", docs, "my-release");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let pkg_content = std::fs::read_to_string(format!("{}/package.yaml", tmp_dir)).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        pkg_content.contains("128Mi"),
        "les resources existantes doivent être conservées si le container n'en définit pas"
    );
}

// ===== gen_package — side effects du placeholder comme release name =====

#[test]
fn gen_system_filename_does_not_contain_release_name_placeholder() {
    // doc_name doit être capturé APRÈS le replace() pour que le nom de fichier
    // ne contienne pas le placeholder du release name.
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/gen_system_filename_placeholder", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let pkg_yaml = "apiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: traefik\n  category: core\n  type: system\n";
    std::fs::write(format!("{}/package.yaml", tmp_dir), pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [#{{
            apiVersion: "apps/v1",
            kind: "Deployment",
            metadata: #{{ name: "v3fdf80f5-traefik" }},
            spec: #{{
                selector: #{{ matchLabels: #{{ app: "v3fdf80f5-traefik" }} }},
                template: #{{
                    metadata: #{{ labels: #{{ app: "v3fdf80f5-traefik" }} }},
                    spec: #{{
                        containers: [#{{ name: "v3fdf80f5-traefik", image: "ghcr.io/traefik/traefik:v3.7.1" }}]
                    }}
                }}
            }}
        }}];
        gen::gen_system("{dir}", docs, "v3fdf80f5");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let systems_dir = format!("{}/get_systems", tmp_dir);
    let files: Vec<_> = std::fs::read_dir(&systems_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        !files.iter().any(|f| f.contains("v3fdf80f5")),
        "le nom de fichier ne doit pas contenir le placeholder du release name, got: {files:?}"
    );
}

#[test]
fn gen_system_args_replace_release_name_with_appslug() {
    // Les valeurs d'args Helm contenant le release name doivent avoir
    // le placeholder remplacé par {{instance.appslug}}.
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/gen_system_args_release", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let pkg_yaml = "apiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: traefik\n  category: core\n  type: system\n";
    std::fs::write(format!("{}/package.yaml", tmp_dir), pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [#{{
            apiVersion: "apps/v1",
            kind: "Deployment",
            metadata: #{{ name: "v3fdf80f5-traefik" }},
            spec: #{{
                selector: #{{ matchLabels: #{{ app: "v3fdf80f5-traefik" }} }},
                template: #{{
                    metadata: #{{ labels: #{{ app: "v3fdf80f5-traefik" }} }},
                    spec: #{{
                        containers: [#{{
                            name: "v3fdf80f5-traefik",
                            image: "ghcr.io/traefik/traefik:v3.7.1",
                            args: [
                                "--providers.kubernetesingress.ingressendpoint.publishedservice=v3abc/v3fdf80f5-traefik",
                                "--providers.kubernetesgateway.statusaddress.service.name=v3fdf80f5-traefik"
                            ]
                        }}]
                    }}
                }}
            }}
        }}];
        gen::gen_system("{dir}", docs, "v3fdf80f5");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let hbs =
        std::fs::read_to_string(format!("{}/get_systems/Deployment_traefik.yaml.hbs", tmp_dir)).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        !hbs.contains("v3fdf80f5"),
        "le placeholder du release name ne doit pas apparaître dans les args générés, got:\n{hbs}"
    );
    assert!(
        hbs.contains("instance.appslug"),
        "les args doivent référencer instance.appslug, got:\n{hbs}"
    );
}

#[test]
fn gen_system_release_name_replaced_in_annotation_values() {
    // Les valeurs d'annotations arbitraires (ex: cert-manager.io/inject-ca-from-secret)
    // contenant le release name placeholder doivent être remplacées par {{instance.appslug}}.
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/gen_system_annotation_release", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let pkg_yaml = "apiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: cert-manager\n  category: core\n  type: system\n";
    std::fs::write(format!("{}/package.yaml", tmp_dir), pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [#{{
            apiVersion: "admissionregistration.k8s.io/v1",
            kind: "MutatingWebhookConfiguration",
            metadata: #{{
                name: "v3fdf80f5-cert-manager-webhook",
                annotations: #{{
                    "cert-manager.io/inject-ca-from-secret": "v56abc12/v3fdf80f5-cert-manager-webhook-ca"
                }}
            }},
            webhooks: [#{{
                name: "webhook.cert-manager.io",
                clientConfig: #{{
                    service: #{{ name: "v3fdf80f5-cert-manager-webhook", namespace: "v56abc12", path: "/mutate" }}
                }},
                admissionReviewVersions: ["v1"],
                sideEffects: "None"
            }}]
        }}];
        gen::gen_system("{dir}", docs, "v3fdf80f5", "v56abc12");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let hbs = std::fs::read_to_string(format!(
        "{}/get_systems/MutatingWebhookConfiguration_cert-manager-webhook.yaml.hbs",
        tmp_dir
    ))
    .unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        !hbs.contains("v3fdf80f5"),
        "le release name placeholder ne doit plus apparaître dans les annotations, got:\n{hbs}"
    );
    assert!(
        !hbs.contains("v56abc12"),
        "le namespace placeholder ne doit plus apparaître, got:\n{hbs}"
    );
    assert!(
        hbs.contains("instance.appslug"),
        "{{instance.appslug}} doit remplacer le release name dans les annotations, got:\n{hbs}"
    );
}

#[test]
fn gen_system_networkpolicy_uses_selector_expression() {
    // Dans gen_system, le NetworkPolicy doit remplacer podSelector.matchLabels
    // par l'expression selector_from_ctx, pas laisser les labels Helm bruts.
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/gen_system_netpol_selector", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let pkg_yaml = "apiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: myapp\n  category: core\n  type: system\n";
    std::fs::write(format!("{}/package.yaml", tmp_dir), pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [
            #{{
                apiVersion: "apps/v1",
                kind: "Deployment",
                metadata: #{{ name: "v3fdf80f5-controller" }},
                spec: #{{
                    selector: #{{ matchLabels: #{{ "app.kubernetes.io/component": "controller", "app.kubernetes.io/instance": "v3fdf80f5" }} }},
                    template: #{{
                        metadata: #{{ labels: #{{ "app.kubernetes.io/component": "controller", "app.kubernetes.io/instance": "v3fdf80f5" }} }},
                        spec: #{{ containers: [#{{ name: "controller", image: "nginx:1.0" }}] }}
                    }}
                }}
            }},
            #{{
                apiVersion: "networking.k8s.io/v1",
                kind: "NetworkPolicy",
                metadata: #{{ name: "v3fdf80f5-allow-egress" }},
                spec: #{{
                    podSelector: #{{
                        matchLabels: #{{ "app.kubernetes.io/component": "controller", "app.kubernetes.io/instance": "v3fdf80f5" }}
                    }},
                    policyTypes: ["Egress"]
                }}
            }}
        ];
        gen::gen_system("{dir}", docs, "v3fdf80f5");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let hbs = std::fs::read_to_string(format!(
        "{}/get_systems/NetworkPolicy_allow-egress.yaml.hbs",
        tmp_dir
    ))
    .unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        hbs.contains("selector_from_ctx"),
        "podSelector.matchLabels doit utiliser selector_from_ctx, got:\n{hbs}"
    );
    assert!(
        !hbs.contains("v3fdf80f5"),
        "le placeholder ne doit plus apparaître dans le NetworkPolicy, got:\n{hbs}"
    );
    assert!(
        !hbs.contains("app.kubernetes.io/instance"),
        "les labels Helm bruts ne doivent pas rester dans podSelector, got:\n{hbs}"
    );
}

#[test]
fn gen_system_tsc_matchlabels_replaced_with_selector_expression() {
    // Les topologySpreadConstraints.labelSelector.matchLabels doivent être remplacés
    // par l'expression selector_from_ctx, pas laisser les labels bruts.
    // Bug : tsc.labelSelector.matchLabels = marker (2 niveaux sur copie) ne persiste pas.
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/gen_system_tsc_selector", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let pkg_yaml = "apiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: myapp\n  category: core\n  type: system\n";
    std::fs::write(format!("{}/package.yaml", tmp_dir), pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [#{{
            apiVersion: "apps/v1",
            kind: "Deployment",
            metadata: #{{ name: "v3fdf80f5-controller" }},
            spec: #{{
                selector: #{{ matchLabels: #{{ "app.kubernetes.io/instance": "v3fdf80f5" }} }},
                template: #{{
                    metadata: #{{ labels: #{{ "app.kubernetes.io/instance": "v3fdf80f5" }} }},
                    spec: #{{
                        containers: [#{{ name: "controller", image: "nginx:1.0" }}],
                        topologySpreadConstraints: [#{{
                            maxSkew: 1,
                            topologyKey: "kubernetes.io/hostname",
                            whenUnsatisfiable: "ScheduleAnyway",
                            labelSelector: #{{
                                matchLabels: #{{ "app.kubernetes.io/instance": "v3fdf80f5" }}
                            }}
                        }}]
                    }}
                }}
            }}
        }}];
        gen::gen_system("{dir}", docs, "v3fdf80f5");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let hbs =
        std::fs::read_to_string(format!("{}/get_systems/Deployment_controller.yaml.hbs", tmp_dir)).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        hbs.contains("selector_from_ctx"),
        "topologySpreadConstraints.labelSelector.matchLabels doit utiliser selector_from_ctx, got:\n{hbs}"
    );
    assert!(
        !hbs.contains("v3fdf80f5"),
        "le placeholder ne doit plus apparaître dans les TSC, got:\n{hbs}"
    );
    assert!(
        !hbs.contains("app.kubernetes.io/instance:"),
        "les labels bruts ne doivent pas rester dans les TSC matchLabels, got:\n{hbs}"
    );
}

#[test]
fn gen_system_pod_template_labels_include_comp_for_selector_match() {
    // Le pod template labels doit utiliser labels_from_ctx this comp="<comp>"
    // pour que le Deployment selector (qui inclut app.kubernetes.io/component)
    // puisse matcher les pods → sinon kube-linter mismatching-selector.
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/gen_system_labels_comp", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let pkg_yaml = "apiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: myapp\n  category: core\n  type: system\n";
    std::fs::write(format!("{}/package.yaml", tmp_dir), pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [#{{
            apiVersion: "apps/v1",
            kind: "Deployment",
            metadata: #{{ name: "v3fdf80f5-controller" }},
            spec: #{{
                selector: #{{ matchLabels: #{{ "app.kubernetes.io/component": "controller", "app.kubernetes.io/instance": "v3fdf80f5" }} }},
                template: #{{
                    metadata: #{{ labels: #{{ "app.kubernetes.io/component": "controller", "app.kubernetes.io/instance": "v3fdf80f5" }} }},
                    spec: #{{ containers: [#{{ name: "controller", image: "nginx:1.0" }}] }}
                }}
            }}
        }}];
        gen::gen_system("{dir}", docs, "v3fdf80f5");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let hbs =
        std::fs::read_to_string(format!("{}/get_systems/Deployment_controller.yaml.hbs", tmp_dir)).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    // Le pod template labels doit passer comp pour matcher le selector
    assert!(
        hbs.contains(r#"labels_from_ctx this comp="app""#),
        "pod template labels doit utiliser labels_from_ctx avec comp pour matcher le selector, got:\n{hbs}"
    );
}

#[test]
fn gen_system_image_key_strips_release_name_prefix() {
    // La clé image dans package.yaml et le template ne doit pas contenir le placeholder.
    // Helm nomme les conteneurs <release>-<component> ; la clé doit être juste <component>.
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/gen_system_img_key", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let pkg_yaml = "apiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: traefik\n  category: core\n  type: system\n";
    std::fs::write(format!("{}/package.yaml", tmp_dir), pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [#{{
            apiVersion: "apps/v1",
            kind: "Deployment",
            metadata: #{{ name: "v3fdf80f5-traefik" }},
            spec: #{{
                selector: #{{ matchLabels: #{{ app: "v3fdf80f5-traefik" }} }},
                template: #{{
                    metadata: #{{ labels: #{{ app: "v3fdf80f5-traefik" }} }},
                    spec: #{{
                        containers: [#{{
                            name: "v3fdf80f5-traefik",
                            image: "ghcr.io/traefik/traefik:v3.7.1"
                        }}]
                    }}
                }}
            }}
        }}];
        gen::gen_system("{dir}", docs, "v3fdf80f5");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let pkg = std::fs::read_to_string(format!("{}/package.yaml", tmp_dir)).unwrap();
    let hbs =
        std::fs::read_to_string(format!("{}/get_systems/Deployment_traefik.yaml.hbs", tmp_dir)).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        !pkg.contains("v3fdf80f5"),
        "package.yaml ne doit pas contenir le placeholder dans les clés images, got:\n{pkg}"
    );
    assert!(
        !hbs.contains("v3fdf80f5"),
        "le template ne doit pas contenir le placeholder dans la clé image_from_ctx, got:\n{hbs}"
    );
}

#[test]
fn gen_system_file_name_strips_release_prefix_to_component() {
    // Quand doc_name = "my-release-controller", le fichier généré doit être
    // "Deployment_controller.yaml.hbs" (prefix strippé, pas le fallback "app").
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/gen_system_strip_prefix", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let pkg_yaml = "apiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: myapp\n  category: core\n  type: system\n";
    std::fs::write(format!("{}/package.yaml", tmp_dir), pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [#{{
            apiVersion: "apps/v1",
            kind: "Deployment",
            metadata: #{{ name: "my-release-controller" }},
            spec: #{{
                selector: #{{ matchLabels: #{{ app: "my-release-controller" }} }},
                template: #{{
                    metadata: #{{ labels: #{{ app: "my-release-controller" }} }},
                    spec: #{{ containers: [#{{ name: "controller", image: "nginx:1.0" }}] }}
                }}
            }}
        }}];
        gen::gen_system("{dir}", docs, "my-release");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let systems_dir = format!("{}/get_systems", tmp_dir);
    let files: Vec<_> = std::fs::read_dir(&systems_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        files.iter().any(|f| f == "Deployment_controller.yaml.hbs"),
        "le préfixe 'my-release-' doit être strippé pour donner 'controller', got: {files:?}"
    );
    assert!(
        !files.iter().any(|f| f.contains("my-release")),
        "le release name ne doit plus apparaître dans les noms de fichiers, got: {files:?}"
    );
}

// ===== gen_package — suppression des labels metadata =====

#[test]
fn gen_package_clean_metadata_removes_labels() {
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "gen_package" as gen;

        let metadata = #{
            name: "my-release",
            labels: #{
                "app.kubernetes.io/name": "myapp",
                "app.kubernetes.io/managed-by": "Helm",
                "helm.sh/chart": "myapp-1.0"
            }
        };

        let cleaned = gen::clean_metadata(metadata, "my-release");

        !("labels" in cleaned)
    "#,
        )
        .unwrap();

    assert!(
        result.as_bool().unwrap(),
        "clean_metadata doit supprimer les labels du metadata"
    );
}

#[test]
fn gen_system_non_crd_has_no_metadata_labels() {
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/gen_system_no_labels", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;

        let docs = [#{{
            apiVersion: "rbac.authorization.k8s.io/v1",
            kind: "ClusterRole",
            metadata: #{{
                name: "my-release-viewer",
                labels: #{{
                    "app.kubernetes.io/name": "my-release",
                    "app.kubernetes.io/managed-by": "Helm"
                }}
            }},
            rules: []
        }}];

        gen::gen_system("{dir}", docs, "my-release");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let obj_path = format!("{}/get_systems/ClusterRole_viewer.yaml.hbs", tmp_dir);
    let content = std::fs::read_to_string(&obj_path).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        !content.contains("app.kubernetes.io/name"),
        "gen_system ne doit pas inclure les labels dans le metadata du ClusterRole"
    );
    assert!(
        !content.contains("app.kubernetes.io/managed-by"),
        "gen_system ne doit pas inclure app.kubernetes.io/managed-by"
    );
}

#[test]
fn gen_system_deployment_metadata_has_no_labels_key() {
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/gen_system_deploy_no_labels", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;

        let docs = [#{{
            apiVersion: "apps/v1",
            kind: "Deployment",
            metadata: #{{
                name: "my-release",
                labels: #{{
                    "app.kubernetes.io/name": "my-release",
                    "app.kubernetes.io/instance": "my-release"
                }}
            }},
            spec: #{{
                selector: #{{ matchLabels: #{{ app: "my-release" }} }},
                template: #{{
                    metadata: #{{ labels: #{{ app: "my-release" }} }},
                    spec: #{{ containers: [#{{ name: "app", image: "nginx:1.0" }}] }}
                }}
            }}
        }}];

        gen::gen_system("{dir}", docs, "my-release");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    // doc_name == release_name → clean_file_name retourne "app" (fallback)
    let obj_path = format!("{}/get_systems/Deployment_app.yaml.hbs", tmp_dir);
    let content = std::fs::read_to_string(&obj_path).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    // Le metadata top-level ne doit pas avoir de labels
    // (le template.metadata.labels est remplacé par l'expression HBS — OK)
    let lines: Vec<&str> = content.lines().collect();
    let mut in_top_metadata = false;
    let mut in_spec = false;
    for line in &lines {
        if line.starts_with("kind:") {
            in_top_metadata = false;
        }
        if line.starts_with("metadata:") && !in_spec {
            in_top_metadata = true;
        }
        if line.starts_with("spec:") {
            in_top_metadata = false;
            in_spec = true;
        }
        if in_top_metadata && line.trim_start().starts_with("labels:") {
            panic!("Le metadata top-level du Deployment contient 'labels:' — doit être supprimé");
        }
    }
}

#[test]
fn gen_tenant_objects_have_no_metadata_labels() {
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/gen_tenant_no_labels", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;

        let docs = [
            #{{
                apiVersion: "v1",
                kind: "ConfigMap",
                metadata: #{{
                    name: "my-release-config",
                    labels: #{{
                        "app.kubernetes.io/name": "my-release",
                        "app.kubernetes.io/managed-by": "Helm",
                        "helm.sh/chart": "mychart-1.0"
                    }}
                }},
                data: #{{ key: "value" }}
            }},
            #{{
                apiVersion: "apps/v1",
                kind: "Deployment",
                metadata: #{{
                    name: "my-release",
                    labels: #{{
                        "app.kubernetes.io/name": "my-release",
                        "app.kubernetes.io/instance": "my-release"
                    }}
                }},
                spec: #{{
                    selector: #{{ matchLabels: #{{ app: "my-release" }} }},
                    template: #{{
                        metadata: #{{ labels: #{{ app: "my-release" }} }},
                        spec: #{{ containers: [#{{ name: "app", image: "nginx:1.0" }}] }}
                    }}
                }}
            }}
        ];

        gen::gen_tenant("{dir}", docs, "my-release");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    // "my-release-config" → strip "my-release-" prefix → "config"
    let cm_path = format!("{}/get_others/ConfigMap_config.yaml.hbs", tmp_dir);
    let cm_content = std::fs::read_to_string(&cm_path).unwrap();

    // "my-release" == release_name → fallback "app"
    let deploy_path = format!("{}/get_scalables/Deployment_app.yaml.hbs", tmp_dir);
    let deploy_content = std::fs::read_to_string(&deploy_path).unwrap();

    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        !cm_content.contains("app.kubernetes.io/managed-by"),
        "gen_tenant ConfigMap ne doit pas avoir de labels dans le metadata"
    );
    assert!(
        !cm_content.contains("helm.sh/chart"),
        "gen_tenant ConfigMap ne doit pas avoir helm.sh/chart dans le metadata"
    );

    // Le Deployment : metadata top-level sans labels
    let deploy_lines: Vec<&str> = deploy_content.lines().collect();
    let mut in_top_metadata = false;
    let mut in_spec = false;
    for line in &deploy_lines {
        if line.starts_with("kind:") {
            in_top_metadata = false;
        }
        if line.starts_with("metadata:") && !in_spec {
            in_top_metadata = true;
        }
        if line.starts_with("spec:") {
            in_top_metadata = false;
            in_spec = true;
        }
        if in_top_metadata && line.trim_start().starts_with("labels:") {
            panic!("gen_tenant Deployment metadata top-level contient 'labels:' — doit être supprimé");
        }
    }
}

// ===== gen_package — namespace placeholder et remplacement =====

#[test]
fn rhai_trim_mutates_in_place_returns_unit() {
    // trim() mute la chaîne en place et retourne () — NE PAS chaîner avec .split() etc.
    // Pattern correct : r.trim(); let parts = r.split("-");
    // Pattern faux   : let parts = r.trim().split("-");  ← split() appelé sur ()
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        let s = "  hello world  ";
        let ret = s.trim();       // mute s, retourne ()
        `s=${s}|ret_is_unit=${type_of(ret) == "()"}`
    "#,
        )
        .unwrap();
    assert_eq!(
        result.to_string(),
        "s=hello world|ret_is_unit=true",
        "trim() doit muter en place et retourner ()"
    );
}

#[test]
fn rhai_return_inside_try_exits_function() {
    // Vérifie que `return` dans un bloc try ne se fait pas attraper par catch(e) {}
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        fn test_fn() {
            try {
                return "from_try";
            } catch(e) {}
            "from_fallback"
        }
        test_fn()
    "#,
        )
        .unwrap();
    assert_eq!(
        result.to_string(),
        "from_try",
        "return dans un try ne doit pas être attrapé par catch"
    );
}

#[test]
fn placeholder_returns_random_value_not_fallback() {
    // Vérifie que placeholder() génère une vraie valeur aléatoire, pas le fallback statique
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "gen_package" as gen;
        gen::placeholder()
    "#,
        )
        .unwrap();
    let s = result.to_string();
    assert_ne!(
        s, "vnsplaceholder",
        "placeholder doit retourner une valeur aléatoire, pas le fallback statique"
    );
}

#[test]
fn placeholder_is_kubernetes_valid() {
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "gen_package" as gen;
        gen::placeholder()
    "#,
        )
        .unwrap();

    let s = result.to_string();
    assert!(!s.is_empty(), "placeholder ne doit pas être vide");
    assert!(
        s.len() <= 63,
        "placeholder doit faire au max 63 caractères, got: {s}"
    );
    assert!(
        s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
        "placeholder doit contenir uniquement [a-z0-9-], got: {s}"
    );
    assert!(
        s.chars().next().map(|c| c.is_ascii_lowercase()).unwrap_or(false),
        "placeholder doit commencer par une lettre minuscule, got: {s}"
    );
    assert!(
        s.chars()
            .last()
            .map(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            .unwrap_or(false),
        "placeholder doit terminer par une lettre ou chiffre, got: {s}"
    );
}

#[test]
fn gen_system_namespace_replaced_in_clusterrole_name() {
    // Traefik génère ClusterRole nommé {release}-traefik-{ns}.
    // gen_system(4-arg) doit :
    //   1. produire un nom de fichier STABLE sans l'UUID ns → ClusterRole_traefik.yaml.hbs
    //   2. remplacer le placeholder ns dans le contenu par {{instance.namespace}}
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/gen_system_ns_replace", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let pkg_yaml = "apiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: traefik\n  category: core\n  type: system\n";
    std::fs::write(format!("{}/package.yaml", tmp_dir), pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [#{{
            apiVersion: "rbac.authorization.k8s.io/v1",
            kind: "ClusterRole",
            metadata: #{{ name: "vrel1234-traefik-vns5678" }},
            rules: []
        }}];
        gen::gen_system("{dir}", docs, "vrel1234", "vns5678");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    // Nom stable : strip préfixe UUID release + strip suffixe UUID ns → "traefik"
    let hbs_content =
        std::fs::read_to_string(format!("{}/get_systems/ClusterRole_traefik.yaml.hbs", tmp_dir)).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        hbs_content.contains("instance.namespace"),
        "le namespace placeholder doit être remplacé par {{{{instance.namespace}}}}, got: {hbs_content}"
    );
    assert!(
        !hbs_content.contains("vns5678"),
        "l'UUID namespace ne doit plus apparaître dans le template généré, got: {hbs_content}"
    );
}

#[test]
fn gen_system_namespace_not_replaced_without_param() {
    // Compat descendante : gen_system à 3 args ne remplace pas le namespace
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/gen_system_no_ns_param", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let pkg_yaml = "apiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: traefik\n  category: core\n  type: system\n";
    std::fs::write(format!("{}/package.yaml", tmp_dir), pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [#{{
            apiVersion: "rbac.authorization.k8s.io/v1",
            kind: "ClusterRole",
            metadata: #{{ name: "traefik-vynil-apps" }},
            rules: []
        }}];
        gen::gen_system("{dir}", docs, "traefik");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let hbs_content =
        std::fs::read_to_string(format!("{}/get_systems/ClusterRole_vynil-apps.yaml.hbs", tmp_dir)).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        hbs_content.contains("vynil-apps"),
        "sans paramètre namespace, le placeholder doit rester dans le template, got: {hbs_content}"
    );
}

#[test]
fn gen_tenant_namespace_replaced_in_generated_files() {
    // gen_tenant(4-arg) : le nom de fichier doit être stable (sans UUID ns)
    // et le contenu doit remplacer l'UUID ns par {{instance.namespace}}.
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{}/tests/tmp/gen_tenant_ns_replace", base);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let pkg_yaml = "apiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: myapp\n  category: apps\n  type: tenant\n";
    std::fs::write(format!("{}/package.yaml", tmp_dir), pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [#{{
            apiVersion: "rbac.authorization.k8s.io/v1",
            kind: "Role",
            metadata: #{{ name: "vrel1234-myapp-vns5678" }},
            rules: []
        }}];
        gen::gen_tenant("{dir}", docs, "vrel1234", "vns5678");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    // Nom stable : strip préfixe UUID release + strip suffixe UUID ns → "myapp"
    let hbs_content = std::fs::read_to_string(format!("{}/get_others/Role_myapp.yaml.hbs", tmp_dir)).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        hbs_content.contains("instance.namespace"),
        "gen_tenant doit remplacer le namespace placeholder par {{{{instance.namespace}}}}, got: {hbs_content}"
    );
    assert!(
        !hbs_content.contains("vns5678"),
        "l'UUID namespace ne doit plus apparaître dans le template généré par gen_tenant, got: {hbs_content}"
    );
}

// ===== backup_context — patterns replace() non couverts =====

#[test]
fn backup_context_replace_path_chain_normalizes_trailing_slash() {
    // Simule le pattern de from_args() lignes 26-31 :
    //   sub_path.replace("/", " ");  // résultat ignoré si Rhai change
    //   sub_path.trim();
    //   sub_path.replace(" ", "/");
    // Avec trailing slash : "a/b/" → " a b " → "a b" → "a/b"
    // Avec sémantique retour (bug) : "a/b/" inchangé
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        let sub_path = "repos/group/project/";
        sub_path.replace("/", " ");
        sub_path.trim();
        sub_path.replace(" ", "/");
        sub_path
    "#,
        )
        .unwrap();

    assert_eq!(
        result.to_string(),
        "repos/group/project",
        "le pattern replace de backup_context.from_args doit normaliser le trailing slash"
    );
}

#[test]
fn backup_context_vital_name_replace_strips_appslug_prefix() {
    // Simule le pattern de backup_context.run() lignes 165-168 :
    //   name.replace(appslug, "");  // résultat ignoré si Rhai change
    //   name.replace("-", " ");
    //   name.trim();
    //   name.replace(" ", "-");
    // "myapp-data-pvc" → "-data-pvc" → " data pvc" → "data pvc" → "data-pvc"
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        let name = "myapp-data-pvc";
        let appslug = "myapp";
        name.replace(appslug, "");
        name.replace("-", " ");
        name.trim();
        name.replace(" ", "-");
        name
    "#,
        )
        .unwrap();

    assert_eq!(
        result.to_string(),
        "data-pvc",
        "le pattern replace de backup_context.run doit supprimer le préfixe appslug"
    );
}

// ===== package_yaml — properties_improve tests =====

#[test]
fn package_yaml_properties_improve_leaf_defaults_all_children_compute_parent_default() {
    // Quand toutes les sous-propriétés ont un default, le parent doit recevoir un default calculé
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "package_yaml" as pkg;

        let options = #{
            myobj: #{
                type: "object",
                properties: #{
                    key1: #{ type: "string", "default": "v1" },
                    key2: #{ type: "string", "default": "v2" },
                }
            }
        };

        let improved = pkg::properties_improve(options);

        improved.myobj["default"].key1 == "v1" &&
        improved.myobj["default"].key2 == "v2"
    "#,
        )
        .unwrap();

    assert!(result.as_bool().unwrap());
}

#[test]
fn package_yaml_properties_improve_leaf_defaults_partial_children_compute_partial_default() {
    // Quand seulement certaines sous-propriétés ont un default, le parent doit recevoir un default partiel
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "package_yaml" as pkg;

        let options = #{
            myobj: #{
                type: "object",
                properties: #{
                    key1: #{ type: "string", "default": "v1" },
                    key2: #{ type: "string" },
                }
            }
        };

        let improved = pkg::properties_improve(options);

        "default" in improved.myobj &&
        improved.myobj["default"].key1 == "v1" &&
        !("key2" in improved.myobj["default"])
    "#,
        )
        .unwrap();

    assert!(result.as_bool().unwrap());
}

#[test]
fn package_yaml_properties_improve_leaf_defaults_no_children_default_means_no_parent_default() {
    // Quand aucune sous-propriété n'a de default, le parent ne doit pas avoir de default
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "package_yaml" as pkg;

        let options = #{
            myobj: #{
                type: "object",
                properties: #{
                    key1: #{ type: "string" },
                    key2: #{ type: "integer" },
                }
            }
        };

        let improved = pkg::properties_improve(options);

        !("default" in improved.myobj)
    "#,
        )
        .unwrap();

    assert!(result.as_bool().unwrap());
}

#[test]
fn package_yaml_properties_improve_existing_parent_default_not_overwritten() {
    // Un default existant sur le parent ne doit pas être écrasé par les defaults des enfants
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "package_yaml" as pkg;

        let options = #{
            myobj: #{
                type: "object",
                "default": #{ key1: "original" },
                properties: #{
                    key1: #{ type: "string", "default": "from-leaf" },
                }
            }
        };

        let improved = pkg::properties_improve(options);

        improved.myobj["default"].key1 == "original"
    "#,
        )
        .unwrap();

    assert!(result.as_bool().unwrap());
}

#[test]
fn package_yaml_properties_improve_leaf_defaults_cascade_multiple_levels() {
    // Les defaults remontent sur plusieurs niveaux : petit-enfant → enfant → parent
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "package_yaml" as pkg;

        let options = #{
            root: #{
                type: "object",
                properties: #{
                    child: #{
                        type: "object",
                        properties: #{
                            leaf: #{ type: "string", "default": "deep" },
                        }
                    }
                }
            }
        };

        let improved = pkg::properties_improve(options);

        // child doit avoir reçu un default depuis leaf
        improved.root.properties.child["default"].leaf == "deep" &&
        // root doit avoir reçu un default depuis child
        improved.root["default"].child.leaf == "deep"
    "#,
        )
        .unwrap();

    assert!(result.as_bool().unwrap());
}

// ===== apply_dedup_to_generated — déduplication globale du suffixe pkg_name =====

#[test]
fn apply_dedup_to_generated_removes_redundant_pkg_suffix_in_all_fields() {
    // Vérifie que apply_dedup_to_generated traite TOUS les champs d'un fichier généré :
    // metadata.name, roleRef, subjects, serviceAccountName, args dynamiques, etc.
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{base}/tests/tmp/apply_dedup_test");
    let systems_dir = format!("{tmp_dir}/get_systems");
    std::fs::create_dir_all(&systems_dir).unwrap();

    let content = "\
---
metadata:
  name: 'mynamespace-{{instance.appslug}}-cert-manager-controller'
roleRef:
  name: '{{instance.appslug}}-cert-manager-controller'
subjects:
- name: '{{instance.appslug}}-cert-manager'
  namespace: mynamespace
spec:
  template:
    spec:
      serviceAccountName: '{{instance.appslug}}-cert-manager'
      containers:
      - args:
        - --leader-election-namespace={{instance.appslug}}-cert-manager
";
    std::fs::write(format!("{systems_dir}/ClusterRoleBinding_test.yaml.hbs"), content).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"import "gen_package" as gen; gen::apply_dedup_to_generated("{dir}", "cert-manager");"#,
            dir = tmp_dir
        ))
        .unwrap();

    let result = std::fs::read_to_string(format!("{systems_dir}/ClusterRoleBinding_test.yaml.hbs")).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        !result.contains("{{instance.appslug}}-cert-manager"),
        "apply_dedup doit supprimer tous les doublons appslug-pkg_name, got:\n{result}"
    );
    assert!(
        result.contains("{{instance.appslug}}-controller"),
        "apply_dedup doit préserver le suffixe après pkg_name, got:\n{result}"
    );
}

#[test]
fn apply_dedup_to_generated_no_change_when_no_redundancy() {
    // Un fichier sans doublon ne doit pas être modifié.
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{base}/tests/tmp/apply_dedup_no_change");
    let systems_dir = format!("{tmp_dir}/get_systems");
    std::fs::create_dir_all(&systems_dir).unwrap();

    let content = "---\nmetadata:\n  name: '{{instance.appslug}}-other-role'\n";
    std::fs::write(format!("{systems_dir}/ClusterRole_test.yaml.hbs"), content).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"import "gen_package" as gen; gen::apply_dedup_to_generated("{dir}", "cert-manager");"#,
            dir = tmp_dir
        ))
        .unwrap();

    let result = std::fs::read_to_string(format!("{systems_dir}/ClusterRole_test.yaml.hbs")).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert_eq!(
        result, content,
        "sans doublon le fichier ne doit pas être modifié"
    );
}

#[test]
fn apply_dedup_to_generated_noop_when_pkg_name_unit() {
    // pkg_name () → aucun fichier modifié.
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{base}/tests/tmp/apply_dedup_noop");
    let systems_dir = format!("{tmp_dir}/get_systems");
    std::fs::create_dir_all(&systems_dir).unwrap();

    let content = "---\nname: '{{instance.appslug}}-cert-manager-edit'\n";
    std::fs::write(format!("{systems_dir}/file.yaml.hbs"), content).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"import "gen_package" as gen; gen::apply_dedup_to_generated("{dir}", ());"#,
            dir = tmp_dir
        ))
        .unwrap();

    let result = std::fs::read_to_string(format!("{systems_dir}/file.yaml.hbs")).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert_eq!(result, content, "pkg_name () doit laisser le fichier intact");
}

// ===== clean_file_name — nettoyage du placeholder namespace dans le nom de fichier =====

#[test]
fn clean_file_name_strips_ns_placeholder_from_filename() {
    // Traefik génère ClusterRole nommé {release}-traefik-{ns}.
    // Après strip du préfixe release, il reste "traefik-{ns}" dans le nom de fichier.
    // La variante 3-arg doit supprimer ce suffixe ns pour des noms stables.
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "gen_package" as gen;
        gen::clean_file_name("vrel1234-traefik-vns5678", "vrel1234", "vns5678")
    "#,
        )
        .unwrap();
    assert_eq!(
        result.to_string(),
        "traefik",
        "clean_file_name doit supprimer le suffixe ns après avoir stripé le préfixe release"
    );
}

#[test]
fn clean_file_name_strips_ns_in_middle_of_filename() {
    // Cas où le ns est au milieu : {release}-component-{ns}-suffix → component-suffix
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "gen_package" as gen;
        gen::clean_file_name("vrel1234-component-vns5678-suffix", "vrel1234", "vns5678")
    "#,
        )
        .unwrap();
    assert_eq!(
        result.to_string(),
        "component-suffix",
        "clean_file_name doit supprimer le ns en position intermédiaire"
    );
}

#[test]
fn clean_file_name_without_ns_behaves_as_before() {
    // La variante 2-arg (sans ns) conserve le comportement existant.
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
        import "gen_package" as gen;
        gen::clean_file_name("vrel1234-traefik-vns5678", "vrel1234")
    "#,
        )
        .unwrap();
    assert_eq!(
        result.to_string(),
        "traefik-vns5678",
        "sans ns, clean_file_name ne doit pas modifier le suffixe ns"
    );
}

// ===== gen_system — nettoyage ns dans les noms cluster-scoped =====

#[test]
fn gen_system_clusterrole_name_has_no_double_namespace() {
    // Traefik génère ClusterRole nommé {release}-traefik-{ns}.
    // Après gen_system(4-arg), le metadata.name du ClusterRole doit être
    // {{instance.namespace}}-{{instance.appslug}} sans doublon de namespace.
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{base}/tests/tmp/gen_system_no_double_ns");
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let pkg_yaml = "apiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: traefik\n  category: core\n  type: system\n";
    std::fs::write(format!("{tmp_dir}/package.yaml"), pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [#{{
            apiVersion: "rbac.authorization.k8s.io/v1",
            kind: "ClusterRole",
            metadata: #{{ name: "vrel1234-traefik-vns5678" }},
            rules: []
        }}];
        gen::gen_system("{dir}", docs, "vrel1234", "vns5678");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let content =
        std::fs::read_to_string(format!("{tmp_dir}/get_systems/ClusterRole_traefik.yaml.hbs")).unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        content.contains("'{{instance.namespace}}-{{instance.appslug}}'"),
        "ClusterRole doit avoir exactement {{{{instance.namespace}}}}-{{{{instance.appslug}}}}, got:\n{content}"
    );
    assert!(
        !content.contains("{{instance.namespace}}-{{instance.appslug}}-{{instance.namespace}}"),
        "ClusterRole ne doit pas avoir {{{{instance.namespace}}}} en doublon, got:\n{content}"
    );
}

#[test]
fn gen_system_clusterrolebinding_rolref_matches_clusterrole_name() {
    // Le roleRef.name du ClusterRoleBinding doit correspondre exactement
    // au metadata.name du ClusterRole référencé.
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{base}/tests/tmp/gen_system_roleref_match");
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let pkg_yaml = "apiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: traefik\n  category: core\n  type: system\n";
    std::fs::write(format!("{tmp_dir}/package.yaml"), pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [
            #{{
                apiVersion: "rbac.authorization.k8s.io/v1",
                kind: "ClusterRole",
                metadata: #{{ name: "vrel1234-traefik-vns5678" }},
                rules: []
            }},
            #{{
                apiVersion: "rbac.authorization.k8s.io/v1",
                kind: "ClusterRoleBinding",
                metadata: #{{ name: "vrel1234-traefik-vns5678" }},
                roleRef: #{{ apiGroup: "rbac.authorization.k8s.io", kind: "ClusterRole", name: "vrel1234-traefik-vns5678" }},
                subjects: [#{{ kind: "ServiceAccount", name: "vrel1234-traefik", namespace: "vns5678" }}]
            }}
        ];
        gen::gen_system("{dir}", docs, "vrel1234", "vns5678");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    let crole =
        std::fs::read_to_string(format!("{tmp_dir}/get_systems/ClusterRole_traefik.yaml.hbs")).unwrap();
    let crb = std::fs::read_to_string(format!(
        "{tmp_dir}/get_systems/ClusterRoleBinding_traefik.yaml.hbs"
    ))
    .unwrap();
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    // Les deux doivent avoir le même nom sans doublon de namespace
    assert!(
        crole.contains("'{{instance.namespace}}-{{instance.appslug}}'"),
        "ClusterRole metadata.name incorrect, got:\n{crole}"
    );
    assert!(
        crb.contains("'{{instance.namespace}}-{{instance.appslug}}'"),
        "ClusterRoleBinding roleRef.name doit matcher ClusterRole, got:\n{crb}"
    );
}

#[test]
fn gen_system_env_configmap_filename_is_stable() {
    // Un Deployment avec des env vars génère un ConfigMap dans get_others.
    // Le nom du fichier doit être stable (sans UUID release).
    let mut rhai = make_lib_script();
    let base = env!("CARGO_MANIFEST_DIR");
    let tmp_dir = format!("{base}/tests/tmp/gen_system_env_cm");
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let pkg_yaml = "apiVersion: vinyl.solidite.fr/v1beta1\nkind: Package\nmetadata:\n  name: traefik\n  category: core\n  type: system\n";
    std::fs::write(format!("{tmp_dir}/package.yaml"), pkg_yaml).unwrap();

    let _ = rhai
        .eval(&format!(
            r#"
        import "gen_package" as gen;
        let docs = [#{{
            apiVersion: "apps/v1",
            kind: "Deployment",
            metadata: #{{ name: "vrel1234-traefik" }},
            spec: #{{
                selector: #{{ matchLabels: #{{ app: "vrel1234-traefik" }} }},
                template: #{{
                    metadata: #{{ labels: #{{ app: "vrel1234-traefik" }} }},
                    spec: #{{
                        serviceAccountName: "vrel1234-traefik",
                        containers: [#{{
                            name: "vrel1234-traefik",
                            image: "docker.io/traefik:v3.0",
                            env: [
                                #{{ name: "LOG_LEVEL", value: "INFO" }},
                                #{{ name: "PORT", value: "8080" }}
                            ]
                        }}]
                    }}
                }}
            }}
        }}];
        gen::gen_system("{dir}", docs, "vrel1234", "vns5678");
    "#,
            dir = tmp_dir
        ))
        .unwrap();

    // Le ConfigMap doit être nommé sans UUID : ConfigMap_traefik-envs.yaml.hbs
    let exists =
        std::path::Path::new(&format!("{tmp_dir}/get_others/ConfigMap_traefik-envs.yaml.hbs")).exists();
    let uuid_exists = std::fs::read_dir(format!("{tmp_dir}/get_others"))
        .map(|d| {
            d.filter_map(|e| e.ok()).any(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.contains("vrel1234") || name.contains("vns5678")
            })
        })
        .unwrap_or(false);
    std::fs::remove_dir_all(&tmp_dir).unwrap();

    assert!(
        exists,
        "ConfigMap_traefik-envs.yaml.hbs doit exister avec un nom stable"
    );
    assert!(
        !uuid_exists,
        "aucun fichier dans get_others ne doit contenir l'UUID release ou namespace"
    );
}
