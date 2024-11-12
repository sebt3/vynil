import "install_from_dir" as dir;
import "wait" as wait;
fn run(instance, context) {
    import_run("install_scalables_pre", instance, context);
    try {
        let applied_objs = dir::install_allowed(instance, context, `${context.package_dir}/scalables`, context.tenant.namespaces);
        if instance.status != () && instance.status.scalables != () {
            dir::remove(applied_objs, instance.status.scalables, instance.metadata.namespace);
        }
        // TODO: scale a target
        let tmp = instance.set_status_scalables(applied_objs);
        wait::all(applied_objs);
    } catch (e) {
        switch type_of(e) {
            "string" => instance.set_status_scalable_failed(e),
            _ => instance.set_status_scalable_failed(json_encode(e))
        }
        throw e;
    }
    import_run("install_scalables_post", instance, context);
}
