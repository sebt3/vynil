use base64::{Engine as _, engine::general_purpose::STANDARD};
use common::{
    httphandler::http_rhai_register,
    k8smock::{K8sJukeBoxMock, k8smock_rhai_register, oci_mock_rhai_register},
    rhaihandler::Script,
};
use rhai::Dynamic;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

const PROJECT_ID: i64 = 99991;
const PROJECT_PATH: &str = "my-group/my-project";
const REGISTRY: &str = "registry.gitlab.com";
const PULL_SECRET: &str = "gitlab-registry-pull";

fn make_gitlab_scan_script(k8s_mocks: Vec<Dynamic>) -> (Script, Arc<Mutex<Vec<Dynamic>>>) {
    let base = env!("CARGO_MANIFEST_DIR");
    let created = Arc::new(Mutex::new(vec![]));
    let mut script = Script::new_core(vec![
        format!("{base}/scripts/boxes"),
        format!("{base}/scripts/lib"),
    ]);
    oci_mock_rhai_register(&mut script.engine);
    http_rhai_register(&mut script.engine);
    k8smock_rhai_register(&mut script.engine, k8s_mocks, created.clone());
    (script, created)
}

fn secret_mock(token: &str) -> Dynamic {
    let auth = STANDARD.encode(format!("k8s:{token}"));
    let dockerconfig = serde_json::json!({
        "auths": { REGISTRY: { "auth": auth } }
    });
    let dockerconfig_b64 = STANDARD.encode(dockerconfig.to_string());
    let json = serde_json::json!({
        "kind": "Secret",
        "metadata": { "name": PULL_SECRET, "namespace": "vynil-system" },
        "data": { ".dockerconfigjson": dockerconfig_b64 }
    });
    serde_json::from_str(&serde_json::to_string(&json).unwrap()).unwrap()
}

fn jukebox(url: &str, with_secret: bool) -> K8sJukeBoxMock {
    let mut spec = serde_json::json!({
        "source": { "gitlab": { "url": url, "registry": REGISTRY, "project": PROJECT_PATH } },
        "maturity": "alpha",
        "schedule": "0 3 * * *"
    });
    if with_secret {
        spec["pull_secret"] = serde_json::json!(PULL_SECRET);
    }
    let json = serde_json::json!({
        "kind": "JukeBox",
        "metadata": { "name": "test-gitlab-box" },
        "spec": spec
    });
    K8sJukeBoxMock {
        obj: serde_json::from_str(&serde_json::to_string(&json).unwrap()).unwrap(),
    }
}

fn run_scan(script: &mut Script, jb: K8sJukeBoxMock) -> common::Result<Dynamic> {
    let base = env!("CARGO_MANIFEST_DIR");
    script.ctx.set_value("box", jb);
    let args = serde_json::json!({ "namespace": "vynil-system", "filter": null });
    script.set_dynamic("args", &args);
    script.run_file(&PathBuf::from(format!("{base}/scripts/boxes/scan.rhai")))
}

fn project_search_response() -> serde_json::Value {
    serde_json::json!([{ "id": PROJECT_ID, "path_with_namespace": PROJECT_PATH }])
}

fn repo_list_response() -> serde_json::Value {
    serde_json::json!([{
        "location": format!("{REGISTRY}/{PROJECT_PATH}/my-image")
    }])
}

fn repos_path() -> String {
    format!("/api/v4/projects/{PROJECT_ID}/registry/repositories")
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn scan_gitlab_resolves_project_by_id() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects"))
        .respond_with(ResponseTemplate::new(200).set_body_json(project_search_response()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(repos_path()))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-total-pages", "1")
                .set_body_json(repo_list_response()),
        )
        .mount(&server)
        .await;

    let (mut script, _) = make_gitlab_scan_script(vec![secret_mock("testtoken")]);
    let result = run_scan(&mut script, jukebox(&server.uri(), true));
    assert!(result.is_ok(), "GitLab scan should succeed: {:?}", result.err());
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_gitlab_project_not_found_fails() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;

    let (mut script, _) = make_gitlab_scan_script(vec![secret_mock("testtoken")]);
    let result = run_scan(&mut script, jukebox(&server.uri(), true));
    assert!(result.is_err(), "scan should fail when project is not found");
    let msg = format!("{:?}", result.err());
    assert!(
        msg.contains("not found"),
        "error should mention 'not found', got: {msg}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_gitlab_wrong_project_in_results_fails() {
    let server = MockServer::start().await;
    // Search returns a different project — should not match
    Mock::given(method("GET"))
        .and(path("/api/v4/projects"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "id": PROJECT_ID, "path_with_namespace": "other-group/other-project" }
        ])))
        .mount(&server)
        .await;

    let (mut script, _) = make_gitlab_scan_script(vec![secret_mock("testtoken")]);
    let result = run_scan(&mut script, jukebox(&server.uri(), true));
    assert!(
        result.is_err(),
        "scan should fail when path_with_namespace does not match"
    );
    let msg = format!("{:?}", result.err());
    assert!(
        msg.contains("not found"),
        "error should mention 'not found', got: {msg}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_gitlab_registry_api_forbidden_fails() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects"))
        .respond_with(ResponseTemplate::new(200).set_body_json(project_search_response()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(repos_path()))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(serde_json::json!({"message": "403 Forbidden"})),
        )
        .mount(&server)
        .await;

    let (mut script, _) = make_gitlab_scan_script(vec![secret_mock("testtoken")]);
    let result = run_scan(&mut script, jukebox(&server.uri(), true));
    assert!(result.is_err(), "scan should fail on HTTP 403 from registry API");
    let msg = format!("{:?}", result.err());
    assert!(msg.contains("403"), "error should mention 403, got: {msg}");
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_gitlab_no_pull_secret_fails() {
    let server = MockServer::start().await;
    let (mut script, _) = make_gitlab_scan_script(vec![]);
    let result = run_scan(&mut script, jukebox(&server.uri(), false));
    assert!(result.is_err(), "scan should fail without pull_secret");
    let msg = format!("{:?}", result.err());
    assert!(
        msg.contains("pull_secret"),
        "error should mention pull_secret, got: {msg}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_gitlab_search_api_error_fails() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(serde_json::json!({"message": "401 Unauthorized"})),
        )
        .mount(&server)
        .await;

    let (mut script, _) = make_gitlab_scan_script(vec![secret_mock("badtoken")]);
    let result = run_scan(&mut script, jukebox(&server.uri(), true));
    assert!(result.is_err(), "scan should fail on HTTP 401 from search API");
    let msg = format!("{:?}", result.err());
    assert!(msg.contains("401"), "error should mention 401, got: {msg}");
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_gitlab_pagination_fetches_all_pages() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/projects"))
        .respond_with(ResponseTemplate::new(200).set_body_json(project_search_response()))
        .mount(&server)
        .await;
    // Page 2 — more specific matcher, registered first so wiremock checks it first
    Mock::given(method("GET"))
        .and(path(repos_path()))
        .and(query_param("page", "2"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-total-pages", "2")
                .set_body_json(serde_json::json!([
                    {"location": format!("{REGISTRY}/{PROJECT_PATH}/image-b")}
                ])),
        )
        .mount(&server)
        .await;
    // Page 1 (no page param) — broader matcher
    Mock::given(method("GET"))
        .and(path(repos_path()))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-total-pages", "2")
                .set_body_json(serde_json::json!([
                    {"location": format!("{REGISTRY}/{PROJECT_PATH}/image-a")}
                ])),
        )
        .mount(&server)
        .await;

    let (mut script, _) = make_gitlab_scan_script(vec![secret_mock("testtoken")]);
    let result = run_scan(&mut script, jukebox(&server.uri(), true));
    assert!(
        result.is_ok(),
        "paginated GitLab scan should succeed: {:?}",
        result.err()
    );
}
