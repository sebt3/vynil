import "install_from_dir" as dir;
fn run(instance, context) {
    import_run("install_others_pre", instance, context);
    try {
        let applied_objs = dir::install(instance, context, `${context.package_dir}/others`, false, true);
        if instance.status != () && instance.status.others != () {
            dir::remove(applied_objs, instance.status.others, instance.metadata.namespace);
        }
        let tmp = instance.set_status_others(applied_objs);
        for j in applied_objs.filter(|j| j.kind == "Job") {
            log_info(`Waiting for ${j.kind} ${j.name} to finish`);
            let job = get_job(j.namespace, j.name);
            job.wait_done(60*5);
        }
    } catch (e) {
        switch type_of(e) {
            "string" => instance.set_status_other_failed(e),
            _ => instance.set_status_other_failed(json_encode(e))
        }
        throw e;
    }
    import_run("install_others_post", instance, context);
}
