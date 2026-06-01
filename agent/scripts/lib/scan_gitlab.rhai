import "secret_dockerconfigjson" as secret;

fn list_repository(spec, namespace) {
    let registry = spec.source.gitlab.registry;
    let project  = spec.source.gitlab.project;
    let url      = spec.source.gitlab.url;

    // GitLab API requires URL-encoded project path (/ -> %2F)
    let encoded_project = project.replace("/", "%2F");

    let gl = new_http_client(url);
    if "pull_secret" in spec.keys() && spec.pull_secret != "" {
        let auth = secret::get_auth_from(spec.pull_secret, namespace, registry);
        // Both PAT and deploy token are passed as Bearer — requires read_api scope
        gl.add_header_bearer(auth.pass);
    } else {
        throw "GitLab registry requires a pull_secret (deploy token or PAT with read_registry + read_api scopes)";
    }

    let initial = gl.get(`api/v4/projects/${encoded_project}/registry/repositories?per_page=20`);
    if initial.code != 200 {
        throw `Unable to list repositories from ${url}/api/v4/projects/${encoded_project}: HTTP ${initial.code}`;
    }

    // GitLab paginates via X-Total-Pages header
    let total_pages = if "x-total-pages" in initial.headers.keys() {
        parse_int(initial.headers["x-total-pages"])
    } else { 1 };

    // The `location` field is the full OCI image reference (registry/path)
    let result = initial.json.map(|r| r.location);

    for i in 2..(total_pages + 1) {
        let list = gl.get(`api/v4/projects/${encoded_project}/registry/repositories?per_page=20&page=${i}`);
        if list.code != 200 {
            throw `Unable to list repositories from ${url} page ${i}: HTTP ${list.code}`;
        }
        result.append(list.json.map(|r| r.location));
    }

    result
}
