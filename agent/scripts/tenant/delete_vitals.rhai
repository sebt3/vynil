import "wait" as wait;
fn run(instance, context) {
    let ctx = import_run("delete_vitals_pre", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    // TODO : trigger a backup before cleaning
    /*if instance.status != () && instance.status.vitals != () && context.namespace.use_backup {
        let hbs = new_hbs();
        hbs.register_partial_dir(context.template_dir);
        if is_dir(`${context.package_dir}/pods`) {
            hbs.register_partial_dir(`${context.package_dir}/pods`);
        }
        let all = yaml_decode_multi(hbs.render_from("{{> manualbackup.yaml }}", context));
        for obj in all {
            dir::install_allowed_obj(obj, context.tenant.namespaces, instance.metadata.namespace);
        }
    }*/
    // TODO : wait for the backup-job to finish
    if instance.status != () && instance.status.vitals != () {
        let deleted = [];
        for old in instance.status.vitals {
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
    }
    ctx = import_run("delete_vitals_post", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    context
}
