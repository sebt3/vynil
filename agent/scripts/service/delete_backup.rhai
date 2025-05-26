import "install_from_dir" as dir;

fn run(instance, context) {
    let ctx = import_run("delete_backup_pre", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    try {
        context = ctx::run(instance, context, false);
        let ctx = import_run("delete_backup_prepare", instance, context);
        if type_of(ctx) == "map" {
            context = ctx;
        }
        let api = k8s_resource("CronJob", instance.metadata.namespace);
        if api.exist() {
            try {
                let o = api.get_obj(`${instance.appslug}-backups`);
                o.delete();
                deleted.push(o);
            } catch(e) {}
        }
        api = k8s_resource("RoleBinding", instance.metadata.namespace);
        if api.exist() {
            try {
                let o = api.get_obj(`${instance.appslug}-backups`);
                o.delete();
                deleted.push(o);
            } catch(e) {}
        }
        api = k8s_resource("ConfigMap", instance.metadata.namespace);
        if api.exist() {
            try {
                let o = api.get_obj(`${instance.appslug}-backups-scripts`);
                o.delete();
                deleted.push(o);
            } catch(e) {}
        }
    } catch (e) {
    }
    ctx = import_run("delete_backup_post", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    context
}
