import "secret_dockerconfigjson" as secret;

fn list_repository(spec, namespace) {
    let registry = spec.source.gitlab.registry;
    let project  = spec.source.gitlab.project;
    let url      = spec.source.gitlab.url;

    let gl = new_http_client(url);
    if "pull_secret" in spec.keys() && spec.pull_secret != "" {
        let auth = secret::get_auth_from(spec.pull_secret, namespace, registry);
        // Both PAT and deploy token are passed as Bearer — requires read_api scope
        gl.add_header_bearer(auth.pass);
    } else {
        throw "GitLab registry requires a pull_secret (deploy token or PAT with read_registry + read_api scopes)";
    }

    // Resolve project path to numeric ID: the HTTP client (reqwest/url crate) decodes %2F to /,
    // breaking the URL-encoded path form. Using the numeric ID avoids the issue entirely.
    let project_name = project.split("/").pop();
    let search = gl.get(`api/v4/projects?search=${project_name}&per_page=20`);
    if search.code != 200 {
        throw `Unable to search project ${project} on ${url}: HTTP ${search.code}`;
    }
    let matches = search.json.filter(|p| p.path_with_namespace == project);
    if matches.len() < 1 {
        throw `Project ${project} not found on ${url} (check token scope: read_api required)`;
    }
    let project_id = matches[0].id;

    let initial = gl.get(`api/v4/projects/${project_id}/registry/repositories?per_page=20`);
    if initial.code != 200 {
        throw `Unable to list repositories for project ${project} (id: ${project_id}) on ${url}: HTTP ${initial.code}`;
    }

    // GitLab paginates via X-Total-Pages header
    let total_pages = if "x-total-pages" in initial.headers.keys() {
        parse_int(initial.headers["x-total-pages"])
    } else { 1 };

    // The `location` field is the full OCI image reference (registry/path)
    let result = initial.json.map(|r| r.location);

    for i in 2..(total_pages + 1) {
        let list = gl.get(`api/v4/projects/${project_id}/registry/repositories?per_page=20&page=${i}`);
        if list.code != 200 {
            throw `Unable to list repositories from ${url} page ${i}: HTTP ${list.code}`;
        }
        result.append(list.json.map(|r| r.location));
    }

    result
}
