import "install_from_dir" as dir;
import "wait" as wait;
fn run(instance, context) {
    import_run("install_vitals_pre", instance, context);
    try {
        let applied_objs = dir::install_allowed(instance, context, `${context.package_dir}/vitals`, context.tenant.namespaces);
        let added = import_run("install_vitals_add", instance, context);
        switch type_of(added) {
            "map" => {
                if dir::validate_add_item(added) {
                    applied_objs.push(added);
                }
            },
            "array" => {
                applied_objs += added.filter(|i|dir::validate_add_item(i));
            }
        }
        if instance.status != () && instance.status.vitals != () {
            for old in instance.status.vitals {
                if ! applied_objs.some(|n| n.kind == old.kind && n.namespace == old.namespace && n.name == old.name) {
                    log_warn(`Not deleting vitals ${old.kind} ${old.name} !`);
                    applied_objs.push(old);
                }
            }
        }
        let tmp = instance.set_status_vitals(applied_objs);
        wait::all(applied_objs);
    } catch (e) {
        switch type_of(e) {
            "string" => instance.set_status_vital_failed(e),
            _ => instance.set_status_vital_failed(json_encode(e))
        }
        throw e;
    }
    import_run("install_vitals_post", instance, context);
}
