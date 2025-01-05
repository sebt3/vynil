import "copy_dir" as copy;
import "package_yaml" as pck;
fn improve_package(src, dest) {
    if ! is_file(src) {
        throw src+" doesnt exist";
    }
    let yaml = yaml_decode(file_read(src));
    yaml = pck::validate(yaml);
    file_write(dest, yaml_encode(yaml));
}
fn build(args) {
    create_dir(args.temp);
    improve_package(args.source+"/package.yaml", args.temp+"/package.yaml");
    // use Rust package.yaml loading for final validation
    let valid = read_package_yaml(args.temp+"/package.yaml");
    valid.validate_options();
    // Duplicate supported directory
    for item in read_dir(args.source) {
        let base = basename(item);
        if base in ["systems", "crds"] && is_dir(item) && valid.metadata.type == "system" {
            copy::dir_exts(item, args.temp+"/"+base, [".hbs", ".yaml", ".yml"]);
        } else if base in ["vitals", "scalables", "others"] && is_dir(item) && valid.metadata.type == "tenant" {
            copy::dir_exts(item, args.temp+"/"+base, [".hbs", ".yaml", ".yml"]);
        } else if base == "pods" && is_dir(item) && valid.metadata.type == "tenant" {
            copy::dir_exts(item, args.temp+"/"+base, [".hbs"]);
        } else if base == "handlebars" && is_dir(item) {
            for sub in read_dir(item) {
                let subbase = basename(sub);
                if subbase == "partials"  && is_dir(sub) {
                    copy::dir_exts(sub, `${args.temp}/${base}/${subbase}`, [".hbs"]);
                } else if subbase == "helpers"  && is_dir(sub) {
                    copy::dir_exts(sub, `${args.temp}/${base}/${subbase}`, [".rhai"]);
                }
            }
        } else if base in ["scripts"] && is_dir(item) {
            copy::dir_exts(item, args.temp+"/"+base, [".rhai", ".sh"], true);
        } else if base == "tofu" && is_dir(item) {
            copy::dir_exts(item, args.temp+"/"+base, [".tf", ".tpl", ".tftpl"], true);
        } else if base == "files" && is_dir(item) {
            copy::dir_all(item, args.temp+"/"+base, true);
        }
    }
    valid
}

fn run(args) {
    let valid = build(args);
    let reg = new_registry(args.registry, args.username, args.password);
    let version = semver_from("0.1.0");
    if args.tag != "" {
        version = semver_from(args.tag);
    } else {
        try {
            let tags = reg.list_tags(args.repository)
                .filter(|v| try{let x = semver_from(v);true}catch(e){false})
                .map(|v| semver_from(v));
            tags.sort(|a,b| if a==b {0} else if a<b {1} else {-1});
            if tags.len() > 0 {
                version = tags[0];
            }
        } catch(e) {}
        switch args.build {
            "major" => version.inc_major(),
            "minor" => version.inc_minor(),
            "patch" => version.inc_patch(),
            "beta"  => version.inc_beta(),
            "alpha" => version.inc_alpha(),
        }
    }
    let annotations = #{
        "fr.solidite.vynil.metadata": json_encode(valid.metadata),
        "fr.solidite.vynil.requirements": json_encode(valid.requirements),
    };
    if valid.options != () {
        annotations["fr.solidite.vynil.options"] = json_encode(valid.options);
    }
    if valid.value_script != () && valid.value_script != "" {
        annotations["fr.solidite.vynil.value_script"] = json_encode(valid.value_script);
    }
    try {
        reg.push_image(args.temp, args.repository, version.to_string(), annotations);
    } catch(e) {
        log_error(`Pushing image failed with: ${e}`);
    }
}
