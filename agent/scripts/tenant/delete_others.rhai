fn run(instance, context) {
    let ctx = import_run("delete_others_pre", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    if instance.status != () && instance.status.others != () {
        let deleted = [];
        for old in instance.status.others {
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
    ctx = import_run("delete_others_post", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    context
}
