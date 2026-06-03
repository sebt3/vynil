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

// ── dir_exts ─────────────────────────────────────────────────────────────────

#[test]
fn dir_exts_copies_matching_files() {
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    fs::write(src.path().join("a.yaml"), "yaml").unwrap();
    fs::write(src.path().join("b.json"), "json").unwrap();
    fs::write(src.path().join("c.txt"), "txt").unwrap();

    let src_path = src.path().to_str().unwrap().to_string();
    let dst_path = dst.path().to_str().unwrap().to_string();

    let mut rhai = make_lib_script();
    let _ = rhai
        .eval(&format!(
            r#"
        import "copy_dir" as cd;
        cd::dir_exts("{src_path}", "{dst_path}", [".yaml", ".json"]);
    "#
        ))
        .unwrap();

    assert!(dst.path().join("a.yaml").exists(), "a.yaml should be copied");
    assert!(dst.path().join("b.json").exists(), "b.json should be copied");
    assert!(!dst.path().join("c.txt").exists(), "c.txt must not be copied");
}

#[test]
fn dir_exts_recurses_into_subdirs_when_recursive() {
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let sub = src.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("nested.yaml"), "nested").unwrap();
    fs::write(src.path().join("top.yaml"), "top").unwrap();

    let src_path = src.path().to_str().unwrap().to_string();
    let dst_path = dst.path().to_str().unwrap().to_string();

    let mut rhai = make_lib_script();
    let _ = rhai
        .eval(&format!(
            r#"
        import "copy_dir" as cd;
        cd::dir_exts("{src_path}", "{dst_path}", [".yaml"], true);
    "#
        ))
        .unwrap();

    assert!(
        dst.path().join("top.yaml").exists(),
        "top-level file should be copied"
    );
    assert!(
        dst.path().join("sub").join("nested.yaml").exists(),
        "nested file should be copied when recursive=true (regression: was calling copy_dir_exts instead of dir_exts)"
    );
}

#[test]
fn dir_exts_does_not_recurse_by_default() {
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let sub = src.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("nested.yaml"), "nested").unwrap();
    fs::write(src.path().join("top.yaml"), "top").unwrap();

    let src_path = src.path().to_str().unwrap().to_string();
    let dst_path = dst.path().to_str().unwrap().to_string();

    let mut rhai = make_lib_script();
    let _ = rhai
        .eval(&format!(
            r#"
        import "copy_dir" as cd;
        cd::dir_exts("{src_path}", "{dst_path}", [".yaml"]);
    "#
        ))
        .unwrap();

    assert!(
        dst.path().join("top.yaml").exists(),
        "top-level file should be copied"
    );
    assert!(
        !dst.path().join("sub").join("nested.yaml").exists(),
        "nested file must not be copied when recursive=false"
    );
}

// ── dir_all ──────────────────────────────────────────────────────────────────

#[test]
fn dir_all_copies_all_files() {
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    fs::write(src.path().join("a.yaml"), "yaml").unwrap();
    fs::write(src.path().join("b.sh"), "sh").unwrap();

    let src_path = src.path().to_str().unwrap().to_string();
    let dst_path = dst.path().to_str().unwrap().to_string();

    let mut rhai = make_lib_script();
    let _ = rhai
        .eval(&format!(
            r#"
        import "copy_dir" as cd;
        cd::dir_all("{src_path}", "{dst_path}");
    "#
        ))
        .unwrap();

    assert!(dst.path().join("a.yaml").exists());
    assert!(dst.path().join("b.sh").exists());
}

#[test]
fn dir_all_recurses_into_subdirs_when_recursive() {
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let sub = src.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("deep.yaml"), "deep").unwrap();
    fs::write(src.path().join("root.yaml"), "root").unwrap();

    let src_path = src.path().to_str().unwrap().to_string();
    let dst_path = dst.path().to_str().unwrap().to_string();

    let mut rhai = make_lib_script();
    let _ = rhai
        .eval(&format!(
            r#"
        import "copy_dir" as cd;
        cd::dir_all("{src_path}", "{dst_path}", true);
    "#
        ))
        .unwrap();

    assert!(
        dst.path().join("root.yaml").exists(),
        "root-level file should be copied"
    );
    assert!(
        dst.path().join("sub").join("deep.yaml").exists(),
        "nested file should be copied when recursive=true (regression: was calling copy_dir_all instead of dir_all)"
    );
}

#[test]
fn dir_all_does_not_recurse_by_default() {
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let sub = src.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("deep.yaml"), "deep").unwrap();
    fs::write(src.path().join("root.yaml"), "root").unwrap();

    let src_path = src.path().to_str().unwrap().to_string();
    let dst_path = dst.path().to_str().unwrap().to_string();

    let mut rhai = make_lib_script();
    let _ = rhai
        .eval(&format!(
            r#"
        import "copy_dir" as cd;
        cd::dir_all("{src_path}", "{dst_path}");
    "#
        ))
        .unwrap();

    assert!(dst.path().join("root.yaml").exists());
    assert!(
        !dst.path().join("sub").join("deep.yaml").exists(),
        "nested file must not be copied when recursive=false"
    );
}
