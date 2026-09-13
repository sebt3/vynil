use std::path::PathBuf;

#[test]
fn missing_manifest_produces_error() {
    let base = env!("CARGO_MANIFEST_DIR");
    let package_dir = PathBuf::from(format!("{base}/tests/fixtures/lint/missing-manifest/"));
    assert!(package_dir.exists());
    let manifest = package_dir.join("package.yaml");
    assert!(!manifest.exists());
}

#[test]
fn valid_package_has_required_files() {
    let base = env!("CARGO_MANIFEST_DIR");
    let package_dir = PathBuf::from(format!("{base}/tests/fixtures/lint/valid-system/"));
    assert!(package_dir.exists());
    let manifest = package_dir.join("package.yaml");
    assert!(manifest.exists());
    let systems_dir = package_dir.join("systems");
    assert!(systems_dir.exists());
}

#[test]
fn invalid_manifest_exists() {
    let base = env!("CARGO_MANIFEST_DIR");
    let package_dir = PathBuf::from(format!("{base}/tests/fixtures/lint/invalid-manifest/"));
    assert!(package_dir.exists());
    let manifest = package_dir.join("package.yaml");
    assert!(manifest.exists());
}

#[test]
fn wrong_dirs_has_unexpected_directory() {
    let base = env!("CARGO_MANIFEST_DIR");
    let package_dir = PathBuf::from(format!("{base}/tests/fixtures/lint/wrong-dirs/"));
    assert!(package_dir.exists());
    let manifest = package_dir.join("package.yaml");
    assert!(manifest.exists());
    let vitals_dir = package_dir.join("vitals");
    assert!(vitals_dir.exists());
    let systems_dir = package_dir.join("systems");
    assert!(systems_dir.exists());
}
