// TODO: implement more from: https://storageclass.info/csidrivers/
// https://github.com/storageclass/storageclass.github.io/blob/main/data/csidrivers.yaml
fn get_prefered_sc(scs) {
    let pref = #{
        "many_safe": (),
        "many_cheap": (),
        "many_only": (),
        "only_performance": (),
        "only_cheap": (),
        "only_safe": (),
    };
    if scs.some(|s| s.provisioner == "smb.csi.k8s.io") {
        let smb = scs.find(|s| s.provisioner == "smb.csi.k8s.io").name;
        pref["many_safe"] = smb;
        pref["many_cheap"] = smb;
    }
    if scs.some(|s| s.provisioner == "k8s-sigs.io/nfs-provisioner") {
        let nfs = scs.find(|s| s.provisioner == "k8s-sigs.io/nfs-provisioner").name;
        pref["many_safe"] = nfs;
        pref["many_cheap"] = nfs;
    }
    if scs.some(|s| s.provisioner == "rancher.io/local-path") {
        let lp = scs.find(|s| s.provisioner == "rancher.io/local-path").name;
        pref["only_performance"] = lp;
        pref["only_cheap"] = lp;
    }
    if scs.some(|s| s.provisioner.ends_with("nfs.csi.ceph.com")) {
        let nfs = scs.find(|s| s.provisioner.ends_with("nfs.csi.ceph.com")) .name;
        pref["many_safe"] = nfs;
        pref["many_cheap"] = nfs;
    }
    if scs.some(|s| s.provisioner.ends_with("cephfs.csi.ceph.com")) {
        let cfs = scs.find(|s| s.provisioner.ends_with("cephfs.csi.ceph.com")) .name;
        pref["many_safe"] = cfs;
    }
    if scs.some(|s| s.provisioner.ends_with("rbd.csi.ceph.com")) {
        let rbd = scs.find(|s| s.provisioner.ends_with("rbd.csi.ceph.com")) .name;
        pref["only_safe"] = rbd;
        pref["many_only"] = rbd;
    }
    pref
}

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

fn run(instance, args) {
    let pkg = read_package_yaml(`${args.package_dir}/package.yaml`);
    let scs = k8s_resource("StorageClass").list().items.map(|s| #{ name: s.metadata.name, provisioner: s.provisioner });
    let current = "";
    if instance.status != () && instance.status.tag != () {
        current = instance.status.tag;
    }
    let cluster_config = if is_file(`${args.config_dir}/agent.yaml`) {
        yaml_decode(file_read(`${args.config_dir}/agent.yaml`))
    } else {#{}};
    cluster_config["storage_classes"] = scs;
    let crds = k8s_resource("CustomResourceDefinition").list_meta().items;
    cluster_config["crds"] = crds.map(|c| c.metadata.name);
    if ! ("prefered" in cluster_config) {
        cluster_config["prefered"] = get_prefered_sc(scs);
    }
    if ! ("ha" in cluster_config) {
        let nodes = k8s_resource("Nodes").list_meta().items.map(|c| c.metadata.name);
        cluster_config["ha"] = nodes.len() > 1;
    }

    #{
        cluster: cluster_config,
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
        values: instance.spec.options,
        defaults: get_default(pkg.options),
        package_dir: args.package_dir
    }
}