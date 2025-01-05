import "backup_context" as ctx;
import "install_from_dir" as dir;
import "wait" as wait;

fn run(instance, context) {
    let ctx = import_run("init_from_pre", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    try {
        context = ctx::run(instance, context, true);
        let ctx = import_run("init_from_prepare", instance, context);
        if type_of(ctx) == "map" {
            context = ctx;
        }
        let hbs = new_hbs();
        hbs.register_partial_dir(context.template_dir);
        if is_dir(`${context.package_dir}/pods`) {
            hbs.register_partial_dir(`${context.package_dir}/pods`);
        }
        let all = yaml_decode_multi(hbs.render_from("{{> init_job.yaml }}", context));
        let applied_objs = [];
        for obj in all {
            applied_objs += dir::install_allowed_obj(obj, context.tenant.namespaces, instance.metadata.namespace);
        }
        wait::all(applied_objs);
    } catch (e) {
        switch type_of(e) {
            "string" => instance.set_status_init_failed(e),
            _ => instance.set_status_init_failed(json_encode(e))
        }
        throw e;
    }
    ctx = import_run("init_from_post", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    context
}
