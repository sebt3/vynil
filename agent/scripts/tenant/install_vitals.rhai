import "install_from_dir" as dir;
fn run(instance, context) {
    import_run("install_vitals_pre", instance, context);
    try {
        let applied_objs = dir::install(instance, context, `${context.package_dir}/vitals`, false, true);
        if instance.status != () && instance.status.vitals != () {
            for old in instance.status.vitals {
                if ! applied_objs.some(|n| n.kind == old.kind && n.namespace == old.namespace && n.name == old.name) {
                    log_warn(`Not deleting vitals ${old.kind} ${old.name} !`);
                    applied_objs.push(old);
                }
            }
        }
        let tmp = instance.set_status_vitals(applied_objs);
    } catch (e) {
        switch type_of(e) {
            "string" => instance.set_status_vital_failed(e),
            _ => instance.set_status_vital_failed(json_encode(e))
        }
        throw e;
    }
    import_run("install_vitals_post", instance, context);
}
