use common::{
    httpmock::httpmock_rhai_register,
    jukebox::JukeBoxDef,
    jukebox_file::{FileJukeBox, FileScanSpec, file_jukebox_rhai_register},
    k8smock::oci_mock_rhai_register,
    rhaihandler::Script,
};
use std::path::PathBuf;
use tempfile::TempDir;

fn make_file_scan_script() -> Script {
    let base = env!("CARGO_MANIFEST_DIR");
    let mut script = Script::new_core(vec![
        format!("{base}/scripts/boxes"),
        format!("{base}/scripts/lib"),
    ]);
    oci_mock_rhai_register(&mut script.engine);
    httpmock_rhai_register(&mut script.engine, vec![]);
    file_jukebox_rhai_register(&mut script.engine);
    script
}

#[test]
fn file_scan_runs_without_error() {
    let base = env!("CARGO_MANIFEST_DIR");
    let cache_dir = TempDir::new().unwrap();
    let mut script = make_file_scan_script();

    let spec = FileScanSpec {
        source: Some(JukeBoxDef::List(vec!["docker.io/sebt3/testpkg".to_string()])),
        pull_secret: None,
    };
    let file_box = FileJukeBox::new(spec, cache_dir.path().to_path_buf());
    script.ctx.set_value("box", file_box);
    script.set_dynamic(
        "args",
        &serde_json::json!({
            "file_scan": true,
            "cache_dir": cache_dir.path().to_string_lossy(),
            "script_dir": format!("{base}/scripts"),
            "filter": null,
            "namespace": "",
        }),
    );

    let result = script.run_file(&PathBuf::from(format!("{base}/scripts/boxes/scan.rhai")));
    assert!(
        result.is_ok(),
        "scan.rhai in file_scan mode should succeed: {:?}",
        result.err()
    );
}

#[test]
fn file_scan_creates_index_yaml() {
    let base = env!("CARGO_MANIFEST_DIR");
    let cache_dir = TempDir::new().unwrap();
    let mut script = make_file_scan_script();

    let spec = FileScanSpec {
        source: Some(JukeBoxDef::List(vec!["docker.io/sebt3/testpkg".to_string()])),
        pull_secret: None,
    };
    let file_box = FileJukeBox::new(spec, cache_dir.path().to_path_buf());
    script.ctx.set_value("box", file_box);
    script.set_dynamic(
        "args",
        &serde_json::json!({
            "file_scan": true,
            "cache_dir": cache_dir.path().to_string_lossy(),
            "script_dir": format!("{base}/scripts"),
            "filter": null,
            "namespace": "",
        }),
    );

    let _ = script
        .run_file(&PathBuf::from(format!("{base}/scripts/boxes/scan.rhai")))
        .unwrap();

    assert!(
        cache_dir.path().join("index.yaml").exists(),
        "index.yaml should be created after file_scan run"
    );
}
