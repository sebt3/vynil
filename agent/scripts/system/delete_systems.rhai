// Do not delete these at first, and only in this order
const LAST = [
    "Service",
    "Deployment",
    "ServiceAccount",
    "ClusterRole",
    "ClusterRoleBinding",
    "Role",
    "RoleBinding",
    "Secret",
    "ConfigMap",
    "PersistentVolume",
    "PersistentVolumeClaim",
    "Namespace",
];

fn run(instance, context) {
    let ctx = import_run("delete_systems_pre", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    if instance.status != () && instance.status.systems != () {
        let deleted = [];
        for old in instance.status.systems.filter(|o| !global::LAST.contains(o.kind)) {
            let ns = if old.namespace != () {old.namespace} else {instance.metadata.namespace};
            let api = k8s_resource(old.kind, ns);
            if api.exist() {
                try {
                    let o = api.get_obj(old.name);
                    o.delete();
                    deleted.push(o);
                } catch(e) {}
            }
        }
        for d in deleted {
            log_info(`Waiting for ${d.metadata.name} to be deleted`);
            d.wait_deleted(60*5);
        }
        deleted = [];
        for k in global::LAST {
            for old in instance.status.systems.filter(|o| o.kind==k) {
                let ns = if old.namespace != () {old.namespace} else {instance.metadata.namespace};
                let api = k8s_resource(old.kind, ns);
                if api.exist() {
                    try {
                        let o = api.get_obj(old.name);
                        o.delete();
                        deleted.push(o);
                    } catch(e) {}
                }
            }
        }
        for d in deleted {
            log_info(`Waiting for ${d.metadata.name} to be deleted`);
            d.wait_deleted(60*5);
        }
    }
    ctx = import_run("delete_systems_post", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    context
}
