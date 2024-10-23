import "install_from_dir" as dir;
fn run(instance, context) {
    import_run("install_systems_pre", instance, context);
    try {
        let applied_objs = dir::install(instance, context, `${context.package_dir}/systems`, true, false);
        if instance.status != () && instance.status.systems != () {
            dir::remove(applied_objs, instance.status.systems, instance.metadata.namespace);
        }
        let tmp = instance.set_status_systems(applied_objs);
        for d in applied_objs.filter(|d| d.kind == "Deployment") {
            log_info(`Waiting for ${d.kind} ${d.namespace} ${d.name} to be available`);
            let deploy = get_deployment(d.namespace, d.name);
            deploy.wait_available(60*5);
        }
    } catch (e) {
        switch type_of(e) {
            "string" => instance.set_status_system_failed(e),
            _ => instance.set_status_system_failed(json_encode(e))
        }
        throw e;
    }
    import_run("install_systems_post", instance, context);
}
