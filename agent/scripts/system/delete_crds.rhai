fn run(instance, context) {
    import_run("delete_crds_pre", instance, context);
    let api = k8s_resource("CustomResourceDefinition");
    if instance.status != () && instance.status.crds != () {
        for old in instance.status.crds {
            let old_api = k8s_resource(old.split(".")[0]);
            if old_api.exist() {
                let cnt = old_api.list_meta().items.len();
                if cnt > 0 {
                    log_warn(`Ignoring to delete ${old} as it still have ${cnt} children`);
                } else {
                    api.delete(old);
                }
            }
        }
    }
    import_run("delete_crds_post", instance, context);
}
