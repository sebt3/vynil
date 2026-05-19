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

fn scan_image_with_maturity(image, reg, all_tags, maturity) {
    let tags = all_tags
        .filter(|str| maturity_filter(str, maturity))
        .filter(|v| security_filter(v))
        .map(|v| semver_from(v));
    if tags.len() < 1 { return []; }
    tags.sort(|a, b| if a == b { 0 } else if a < b { 1 } else { -1 });
    let tags = tags.map(|v| v.to_string());

    let iter_tags = tags.shift();
    let last_min_version = ();
    let found = [];

    while iter_tags != () && type_of(iter_tags) == "string" && iter_tags != "" {
        let current = iter_tags;
        iter_tags = ();
        let annotations = reg.get_manifest(image.repository, current).annotations;
        if annotations.keys().contains("fr.solidite.vynil.metadata") && annotations.keys().contains("fr.solidite.vynil.requirements") {
            let cur = #{
                registry: image.registry,
                image: image.repository,
                tag: current,
                metadata: json_decode(annotations["fr.solidite.vynil.metadata"]),
                requirements: json_decode(annotations["fr.solidite.vynil.requirements"]),
            };
            if annotations.keys().contains("fr.solidite.vynil.options") {
                cur["options"] = json_decode(annotations["fr.solidite.vynil.options"]);
            }
            if annotations.keys().contains("fr.solidite.vynil.recommandations") {
                cur["recommandations"] = json_decode(annotations["fr.solidite.vynil.recommandations"]);
            }
            if annotations.keys().contains("fr.solidite.vynil.value_script") {
                cur["value_script"] = annotations["fr.solidite.vynil.value_script"];
            }
            if (type_of(cur["requirements"]) == "array" && cur["requirements"].len() > 0) {
                if (cur["requirements"].some(|i| i.vynil_version != () && security_filter(i.vynil_version) && semver_from(i.vynil_version)>semver_from(vynil_version()))) {
                    if tags.len() >= 1 {iter_tags = tags.shift();}
                    print(`Ignoring ${image.repository} as it requiere a too new version, ${iter_tags}, ${current}, ${tags.len}`);
                } else if (last_min_version != () && cur["requirements"].some(|i| i.minimum_previous_version != () && security_filter(i.minimum_previous_version) && semver_from(i.minimum_previous_version)>=semver_from(last_min_version))) {
                    if tags.len() >= 1 {iter_tags = tags.shift();}
                } else {
                    if cur["requirements"].some(|i| i.minimum_previous_version != () && security_filter(i.minimum_previous_version)) {
                        last_min_version = cur["requirements"].find(|i| i.minimum_previous_version != () && security_filter(i.minimum_previous_version)).minimum_previous_version;
                        if tags.len() >= 1 {iter_tags = tags.shift();}
                    } else {
                        last_min_version = ();
                    }
                    found += cur;
                }
            } else {
                found += cur;
            }
        }
    }
    found
}

fn union_packages(sets) {
    let seen = #{};
    let result = [];
    for pkg in sets.reduce(|acc, x| acc + x, []) {
        if !seen.contains_key(pkg.tag) {
            seen[pkg.tag] = true;
            result.push(pkg);
        }
    }
    result
}

let scan_filter = if "filter" in args.keys() && args.filter != () { args.filter } else { () };
let file_scan   = "file_scan" in args.keys() && args.file_scan == true;

let spec = box.spec;
let use_auth = "pull_secret" in spec.keys() && spec.pull_secret!="" && spec.pull_secret!=();
try {
    let images_list = if "harbor" in spec.source.keys() {
            harbor::list_repository(spec, args.namespace)
        } else if "script" in spec.source.keys() {
            eval(spec.source.script)
        } else {
            spec.source.list
        };
    let list = images_list.map(|image| {
        let tmp = image.split("/");
        let reg = tmp.shift();
        #{registry: reg, repository: tmp.reduce(|sum| if sum=="" {this} else {`${sum}/${this}`}, "")}
    });

    let list = if scan_filter == () {
        list
    } else {
        let filter_parts = scan_filter.split("/");
        let filter_cat   = filter_parts[0];
        let filter_name  = if filter_parts.len() > 1 { filter_parts[1] } else { () };
        let status_pkgs = [];
        try { status_pkgs = box.status.packages; } catch(e) {}
        let matched = status_pkgs
            .filter(|p|
                p.metadata.category == filter_cat &&
                (filter_name == () || p.metadata.name == filter_name)
            )
            .map(|p| #{registry: p.registry, repository: p.image});
        let seen = #{}; let deduped = [];
        for m in matched {
            let k = `${m.registry}/${m.repository}`;
            if k in seen {} else { seen[k] = true; deduped.push(m); }
        }
        if deduped.len() > 0 { deduped } else { list }
    };

    let found = [];
    for image in list {
        let auth = if use_auth {secret::get_auth_from(spec.pull_secret, args.namespace, image.registry);} else {#{user: "", pass: ""}};
        log_info(`Scanning ${image.registry}/${image.repository}`);
        let reg = new_registry(image.registry, auth.user, auth.pass);
        let got_tags = reg.list_tags(image.repository);
        found += scan_image_with_maturity(image, reg, got_tags, spec.maturity);
    }

    if scan_filter != () {
        let tmp = box.set_status_packages_merge(scan_filter, found);
    } else {
        let tmp = box.set_status_updated(found);
    }
} catch(e) {
    if type_of(e) == "string" {
        log_warn(e);
        let tmp = box.set_status_failed(e);
    } else {
        log_warn(json_encode(e));
        let tmp = box.set_status_failed(json_encode(e));
    }
    throw;
};
