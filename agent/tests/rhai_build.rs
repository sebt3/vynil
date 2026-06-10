use common::rhaihandler::Script;

fn make_build_script() -> Script {
    let base = env!("CARGO_MANIFEST_DIR");
    Script::new_mock(
        vec![format!("{base}/scripts/packages"), format!("{base}/scripts/lib")],
        vec![],
        vec![],
        Default::default(),
    )
}

// ── security_filter in build.rhai — TDD ──────────────────────────────────
// build.rhai contains a security_filter function identical to the one in scan.rhai.
// Old code calls log_error() for every non-semver tag, producing ERROR noise.
// New code must return false silently for tags that do not start with a digit or 'v'.

#[test]
fn build_security_filter_cosign_tag_no_error_log() {
    // sha256-*.sig tags (Cosign signatures) must be silently rejected.
    // Old code: log_error("sha256-deadbeef.sig") → mock throws → test fails.
    // New code: starts with 's' → returns false silently → test passes.
    let mut script = make_build_script();
    script.add_code(r#"fn log_error(msg) { throw `unexpected error log: ${msg}`; }"#);
    let result = script.eval(r#"import "build" as b; b::security_filter("sha256-deadbeef.sig")"#);
    assert!(result.is_ok(), "Expected no error log: {:?}", result.err());
    assert!(
        !result.unwrap().as_bool().unwrap(),
        "Expected false for cosign tag"
    );
}

#[test]
fn build_security_filter_trivy_tag_no_error_log() {
    // trivy-* tags must be silently rejected.
    let mut script = make_build_script();
    script.add_code(r#"fn log_error(msg) { throw `unexpected error log: ${msg}`; }"#);
    let result = script.eval(r#"import "build" as b; b::security_filter("trivy--apps-auth")"#);
    assert!(result.is_ok(), "Expected no error log: {:?}", result.err());
    assert!(
        !result.unwrap().as_bool().unwrap(),
        "Expected false for trivy tag"
    );
}

#[test]
fn build_security_filter_latest_tag_no_error_log() {
    // "latest" and other non-versioned tags must be silently rejected.
    let mut script = make_build_script();
    script.add_code(r#"fn log_error(msg) { throw `unexpected error log: ${msg}`; }"#);
    let result = script.eval(r#"import "build" as b; b::security_filter("latest")"#);
    assert!(result.is_ok(), "Expected no error log: {:?}", result.err());
    assert!(!result.unwrap().as_bool().unwrap(), "Expected false for 'latest'");
}

#[test]
fn build_security_filter_semver_tag_accepted() {
    // Valid semver tags (starting with digit) must return true.
    let mut script = make_build_script();
    script.add_code(r#"fn log_error(msg) { throw `unexpected error log: ${msg}`; }"#);
    let result = script.eval(r#"import "build" as b; b::security_filter("1.2.3")"#);
    assert!(result.is_ok(), "Expected success: {:?}", result.err());
    assert!(result.unwrap().as_bool().unwrap(), "Expected true for '1.2.3'");
}

#[test]
fn build_security_filter_v_prefixed_semver_accepted() {
    // Tags starting with 'v' followed by semver must return true.
    let mut script = make_build_script();
    script.add_code(r#"fn log_error(msg) { throw `unexpected error log: ${msg}`; }"#);
    let result = script.eval(r#"import "build" as b; b::security_filter("v2.0.0")"#);
    assert!(result.is_ok(), "Expected success: {:?}", result.err());
    assert!(result.unwrap().as_bool().unwrap(), "Expected true for 'v2.0.0'");
}

#[test]
fn build_security_filter_empty_string_rejected() {
    let mut script = make_build_script();
    script.add_code(r#"fn log_error(msg) { throw `unexpected error log: ${msg}`; }"#);
    let result = script.eval(r#"import "build" as b; b::security_filter("")"#);
    assert!(result.is_ok(), "Expected success: {:?}", result.err());
    assert!(
        !result.unwrap().as_bool().unwrap(),
        "Expected false for empty string"
    );
}
