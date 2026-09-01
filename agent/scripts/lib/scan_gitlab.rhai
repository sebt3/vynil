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

    // GitLab accepts the URL-encoded "namespace/path" directly as the :id path param on every
    // /projects/:id/* endpoint, so we can address the registry endpoint straight away. This
    // used to go through a project-search + path_with_namespace filter to resolve a numeric ID,
    // worked around a suspected %2F-decoding bug — but that search is unscoped (it matches
    // across all of GitLab, not just projects the token can see) and caps at one page of
    // results, so it silently fails to find any project whose name is a common word (e.g.
    // "box"). The encoded-path form was re-verified against the live GitLab API and works.
    // Rhai's `String::replace` mutates in place and returns () — copy first, then mutate the
    // copy, or `project` (used below in error messages) would end up encoded too.
    let encoded_project = project;
    encoded_project.replace("/", "%2F");

    let initial = gl.get(`api/v4/projects/${encoded_project}/registry/repositories?per_page=20`);
    if initial.code != 200 {
        throw `Unable to list repositories for project ${project} on ${url}: HTTP ${initial.code} (check the project path and that the token has read_api + read_registry scope)`;
    }

    // GitLab paginates via the X-Total-Pages header
    let total_pages_header = headers_get(initial.headers, "x-total-pages");
    let total_pages = if total_pages_header == () { 1 } else { parse_int(total_pages_header) };

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
