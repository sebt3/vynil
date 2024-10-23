import "secret_dockerconfigjson" as secret;

fn list_repository(spec, namespace) {
    let registry = spec.source.harbor.registry;
    let project = spec.source.harbor.project;
    let harbor = new_http_client(`https://${registry}`);
    if "pull_secret" in spec && spec.pull_secret!="" {
        let auth = secret::get_auth_from(spec.pull_secret, namespace, registry);
        harbor.add_header_basic(auth.user, auth.pass);
    } else {
        let auth = harbor.get(`service/token?service=harbor-registry&scope=repository:${project}/*:pull`);
        if auth.code != 200 {
            throw "Cannot get anonymous authentication from harbor";
        }
        harbor.add_header_bearer(auth.json.token);
    }
    let list = harbor.get(`api/v2.0/projects/${project}/repositories?page_size=100`);
    if list.code != 200 {
        throw `Unable to list from ${registry}/${project}`;
    }
    list.json.map(|p| `${registry}/${p.name}`)
}
