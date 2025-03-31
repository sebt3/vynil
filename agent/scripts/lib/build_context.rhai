
import "storage_class_enrich" as sce;
fn appslug(pkg, inst) {
    if pkg == inst {
        inst
    } else if `${inst}-${pkg}`.len() > 28 {
        inst.truncate(28);
        inst
    } else {
        `${inst}-${pkg}`
    }
}
fn get_default(def) {
    let r = #{};
    if def != () && type_of(def) == "map" {
        for k in def.keys() {
            r[k] = def[k]["default"];
        }
    }
    r
}

fn get_values(options, defaults) {
    if type_of(options) == "()" {
        defaults
    } else if type_of(defaults) == "map" && type_of(options) == "map" {
        let r = #{};
        for k in defaults.keys() {
            r[k] = get_values(options[k],defaults[k]);
        }
        r
    } else {
        options
    }
}

fn run(instance, args) {
    let pkg = read_package_yaml(`${args.package_dir}/package.yaml`);
    let scs = sce::classes_enrich(k8s_resource("StorageClass").list().items.map(|s| #{ name: s.metadata.name, provisioner: s.provisioner, is_default: s.metadata.annotations != () && s.metadata.annotations["storageclass.kubernetes.io/is-default-class"]=="true" }));
    let current = "";
    let controller = #{};
    try {
        controller = json_decode(args.controller_values);
    } catch {}
    if instance.status != () && instance.status.tag != () {
        current = instance.status.tag;
    }
    let cluster_config = if is_file(`${args.config_dir}/agent.yaml`) {
        yaml_decode(file_read(`${args.config_dir}/agent.yaml`))
    } else {#{}};
    cluster_config["storage_classes"] = scs;
    let crds = k8s_resource("CustomResourceDefinition").list_meta().items;
    cluster_config["crds"] = crds.map(|c| c.metadata.name);
    if ! ("prefered_storage" in cluster_config) {
        cluster_config["prefered_storage"] = sce::get_prefered_sc(scs);
    }
    if ! ("ha" in cluster_config) {
        let nodes = k8s_resource("Nodes").list_meta().items.map(|c| c.metadata.name);
        cluster_config["ha"] = nodes.len() > 1;
    }
    cluster_config["vynil_namespace"] = args.vynil_namespace;
    let defaults = get_default(pkg.options);

    #{
        cluster: cluster_config,
        controller: controller,
        instance: #{
            name: instance.metadata.name,
            namespace: instance.metadata.namespace,
            "package": #{
                name: instance.spec["package"],
                jukebox: instance.spec.jukebox,
                category: instance.spec.category,
                app_version: pkg.metadata.app_version,
                options: pkg.options,
            },
            labels: #{
                "app.kubernetes.io/name": instance.spec["package"],
                "app.kubernetes.io/instance": instance.metadata.name,
                "app.kubernetes.io/managed-by": "vynil",
                "vynil.solidite.fr/owner-namespace": instance.metadata.namespace,
                "vynil.solidite.fr/owner-category": instance.spec.category,
                "vynil.solidite.fr/owner-type": "system",
            },
            selector: #{
                "app.kubernetes.io/name": instance.spec["package"],
                "app.kubernetes.io/instance": instance.metadata.name,
            },
            images: pkg.images,
            resources: pkg.resources,
            current: current,
            requested: args.tag,
            appslug: appslug(instance.spec["package"], instance.metadata.name)
        },
        values: get_values(instance.spec.options, defaults),
        defaults: defaults,
        package_dir: args.package_dir,
        config_dir: args.config_dir,
    }
}