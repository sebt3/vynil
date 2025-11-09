fn run(instance, context) {
    let ctx = import_run("delete_crds_pre", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    let api = k8s_resource("CustomResourceDefinition");
    if instance.status != () && instance.status.crds != () {
        for old in instance.status.crds {
            try {
                let old_api = k8s_resource(old.split(".")[0]);
                if old_api.exist() {
                    let cnt = old_api.list_meta().items.len();
                    if cnt > 0 {
                        log_warn(`Ignoring to delete ${old} as it still have ${cnt} children`);
                    } else {
                        log_info(`Deleting CRD ${old}`);
                        try {
                            let o = api.get_obj(old);
                            o.delete();
                        } catch (e) {}
                    }
                }
            } catch (e) {}
        }
    }
    ctx = import_run("delete_crds_post", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    context
}
