import "scan_harbor" as harbor;
import "secret_dockerconfigjson" as secret;

fn maturity_filter(str, maturity) {
    if maturity == "alpha" {true}
    else if maturity == "beta" {!str.contains("alpha")}
    else {!str.contains("alpha") && !str.contains("beta")}
}

fn security_filter(v) {
    try{
        let x = semver_from(v);
        return true
    }catch(e){
        log_error(v);
        if type_of(e) == "string" {
            log_warn(e);
        } else {
            log_warn(json_encode(e));
        }
    }
    return false
}

let spec = box.spec;
let use_auth = "pull_secret" in spec && spec.pull_secret!="" && spec.pull_secret!=();
try {
    let images_list = if "harbor" in spec.source {
            harbor::list_repository(spec, args.namespace)
        } else if "script" in spec.source {
            eval(spec.source.script)
        } else {
            spec.source.list
        };
    let list = images_list.map(|image| {
        let tmp = image.split("/");
        let reg = tmp.shift();
        #{registry: reg, repository: tmp.reduce(|sum| if sum=="" {this} else {`${sum}/${this}`}, "")}
    });
    let found = [];
    for image in list {
        let auth = if use_auth {secret::get_auth_from(spec.pull_secret, args.namespace, image.registry);} else {#{user: "", pass: ""}};
        log_info(`Scanning ${image.registry}/${image.repository}`);
        let reg = new_registry(image.registry, auth.user, auth.pass);
        // list all tags for this image, sort
        let got_tags = reg.list_tags(image.repository);
        let tags = got_tags
            .filter(|str| maturity_filter(str, spec.maturity))
            .filter(|v| security_filter(v))
            .map(|v| semver_from(v));
        if tags.len() < 1 {continue;} // No found tags, skip
        tags.sort(|a,b| if a==b {0} else if a<b {1} else {-1});
        let tags = tags.map(|v| v.to_string());
        // get the annotations from the highest version
        let annotations = reg.get_manifest(image.repository, tags[0]).annotations;
        if annotations.keys().contains("fr.solidite.vynil.metadata") && annotations.keys().contains("fr.solidite.vynil.requirements") {
            let cur = #{
                registry: image.registry,
                image: image.repository,
                tag: tags[0],
                metadata: json_decode(annotations["fr.solidite.vynil.metadata"]),
                requirements: json_decode(annotations["fr.solidite.vynil.requirements"]),
            };
            if annotations.keys().contains("fr.solidite.vynil.options") {
                cur["options"] = json_decode(annotations["fr.solidite.vynil.options"]);
            }
            if annotations.keys().contains("fr.solidite.vynil.value_script") {
                cur["value_script"] = annotations["fr.solidite.vynil.value_script"];
            }
            found += cur;
        }
    }
    let tmp = box.set_status_updated(found);
} catch(e) {
    if type_of(e) == "string" {
        log_warn(e);
        let tmp = box.set_status_failed(e);
    } else {
        log_warn(json_encode(e));
        let tmp = box.set_status_failed(json_encode(e));
    }
    throw e;
};
