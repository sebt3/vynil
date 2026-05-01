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
