use common::rhaihandler::Script;
use std::fs;

fn make_lib_script() -> Script {
    let base = env!("CARGO_MANIFEST_DIR");
    Script::new_mock(
        vec![format!("{base}/scripts/lib")],
        vec![],
        vec![],
        Default::default(),
    )
}

// ── has_tofu_files ────────────────────────────────────────────────────────────

#[test]
fn has_tofu_files_returns_false_for_nonexistent_dir() {
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
            import "tofu_gen" as tg;
            tg::has_tofu_files("/tmp/vynil_test_nonexistent_dir_xyz")
        "#,
        )
        .unwrap();
    assert!(!result.as_bool().unwrap(), "non-existent dir should return false");
}

#[test]
fn has_tofu_files_returns_false_for_empty_dir() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let mut rhai = make_lib_script();
    rhai.eval(&format!(
        r#"
        import "tofu_gen" as tg;
        tg::has_tofu_files("{path}")
    "#
    ))
    .unwrap()
    .as_bool()
    .map(|v| assert!(!v, "empty dir should return false"))
    .unwrap();
}

#[test]
fn has_tofu_files_returns_false_for_only_generated_files() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("00_vynil_vars.tf"), "").unwrap();
    fs::write(dir.path().join("00_vynil_locals.tf"), "").unwrap();
    let path = dir.path().to_str().unwrap().to_string();

    let mut rhai = make_lib_script();
    let result = rhai
        .eval(&format!(
            r#"
            import "tofu_gen" as tg;
            tg::has_tofu_files("{path}")
        "#
        ))
        .unwrap();
    assert!(
        !result.as_bool().unwrap(),
        "only generated files should return false"
    );
}

#[test]
fn has_tofu_files_returns_true_for_user_tf_file() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("main.tf"), "# user resource").unwrap();
    let path = dir.path().to_str().unwrap().to_string();

    let mut rhai = make_lib_script();
    let result = rhai
        .eval(&format!(
            r#"
            import "tofu_gen" as tg;
            tg::has_tofu_files("{path}")
        "#
        ))
        .unwrap();
    assert!(result.as_bool().unwrap(), "main.tf should return true");
}

#[test]
fn has_tofu_files_returns_false_for_providers_tf_only() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("providers.tf"), "terraform {}").unwrap();
    let path = dir.path().to_str().unwrap().to_string();

    let mut rhai = make_lib_script();
    let result = rhai
        .eval(&format!(
            r#"
            import "tofu_gen" as tg;
            tg::has_tofu_files("{path}")
        "#
        ))
        .unwrap();
    assert!(
        !result.as_bool().unwrap(),
        "providers.tf is generated — should not count as user file"
    );
}

#[test]
fn has_tofu_files_returns_true_when_providers_tf_and_user_file_coexist() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("providers.tf"), "terraform {}").unwrap();
    fs::write(dir.path().join("main.tf"), "# user resource").unwrap();
    let path = dir.path().to_str().unwrap().to_string();

    let mut rhai = make_lib_script();
    let result = rhai
        .eval(&format!(
            r#"
            import "tofu_gen" as tg;
            tg::has_tofu_files("{path}")
        "#
        ))
        .unwrap();
    assert!(
        result.as_bool().unwrap(),
        "main.tf is a user file — should return true"
    );
}

#[test]
fn has_tofu_files_returns_true_when_generated_and_user_files_coexist() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("00_vynil_vars.tf"), "").unwrap();
    fs::write(dir.path().join("00_vynil_locals.tf"), "").unwrap();
    fs::write(dir.path().join("main.tf"), "# user resource").unwrap();
    let path = dir.path().to_str().unwrap().to_string();

    let mut rhai = make_lib_script();
    let result = rhai
        .eval(&format!(
            r#"
            import "tofu_gen" as tg;
            tg::has_tofu_files("{path}")
        "#
        ))
        .unwrap();
    assert!(result.as_bool().unwrap(), "mixed dir should return true");
}

#[test]
fn has_tofu_files_ignores_non_tf_files() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("config.yaml"), "key: value").unwrap();
    fs::write(dir.path().join("README.md"), "docs").unwrap();
    let path = dir.path().to_str().unwrap().to_string();

    let mut rhai = make_lib_script();
    let result = rhai
        .eval(&format!(
            r#"
            import "tofu_gen" as tg;
            tg::has_tofu_files("{path}")
        "#
        ))
        .unwrap();
    assert!(
        !result.as_bool().unwrap(),
        "non-.tf files should not trigger tofu"
    );
}

// ── gen_provider ──────────────────────────────────────────────────────────────

#[test]
fn gen_provider_creates_providers_tf_with_fallback_versions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_str().unwrap().to_string();

    let mut rhai = make_lib_script();
    let _ = rhai
        .eval(&format!(
            r#"
        import "tofu_gen" as tg;
        tg::gen_provider("{path}")
    "#
        ))
        .unwrap();

    let content = fs::read_to_string(dir.path().join("providers.tf")).unwrap();
    assert!(
        content.contains("hashicorp/kubernetes"),
        "should reference kubernetes provider"
    );
    assert!(
        content.contains("gavinbunney/kubectl"),
        "should reference kubectl provider"
    );
    assert!(
        content.contains("kubernetes.default.svc"),
        "should configure in-cluster auth"
    );
    // fallback constraints present when no cache
    assert!(
        content.contains("~> 2.34.0") || content.contains("= 2."),
        "should have a kubernetes version constraint"
    );
}

#[test]
fn gen_provider_overwrites_existing_providers_tf() {
    let dir = tempfile::tempdir().unwrap();
    let stale = "# stale old content with ~> 1.14.0";
    fs::write(dir.path().join("providers.tf"), stale).unwrap();
    let path = dir.path().to_str().unwrap().to_string();

    let mut rhai = make_lib_script();
    let _ = rhai
        .eval(&format!(
            r#"
        import "tofu_gen" as tg;
        tg::gen_provider("{path}")
    "#
        ))
        .unwrap();

    let content = fs::read_to_string(dir.path().join("providers.tf")).unwrap();
    assert_ne!(content, stale, "stale providers.tf must be replaced");
    assert!(
        content.contains("hashicorp/kubernetes"),
        "regenerated file must declare kubernetes provider"
    );
}

// ── provider_constraint ───────────────────────────────────────────────────────

#[test]
fn provider_constraint_returns_fallback_when_cache_absent() {
    let mut rhai = make_lib_script();
    let result = rhai
        .eval(
            r#"
            import "tofu_gen" as tg;
            tg::provider_constraint("hashicorp", "kubernetes", "~> 2.34.0")
        "#,
        )
        .unwrap();
    // When the real plugin cache is absent (test env), fallback is returned
    let s = result.to_string();
    assert!(
        s.contains("2.34") || s.starts_with('='),
        "should return fallback or cached version, got: {s}"
    );
}
