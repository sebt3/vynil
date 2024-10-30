fn run(instance, context) {
    import_run("delete_systems_pre", instance, context);
    if instance.status != () && instance.status.systems != () {
        let deleted = [];
        for old in instance.status.systems {
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
    import_run("delete_systems_post", instance, context);
}
