import "backup_context" as bctx;
import "install_from_dir" as dir;
import "wait" as wait;

fn run(instance, context) {
    let ctx = import_run("init_from_pre", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    try {
        let ctx = import_run("init_from_prepare", instance, context);
        if type_of(ctx) == "map" {
            context = ctx;
        }
        let hbs = new_hbs();
        hbs.register_partial_dir(context.template_dir);
        if is_dir(`${context.package_dir}/pods`) {
            hbs.register_partial_dir(`${context.package_dir}/pods`);
        }
        let all = yaml_decode_multi(hbs.render_from("{{> init_job.yaml }}", bctx::run(instance, context, true)));
        let applied_objs = [];
        for obj in all {
            applied_objs += dir::install_obj(obj, true, instance.metadata.namespace, false);
        }
        wait::all(applied_objs);
    } catch (e) {
        switch type_of(e) {
            "string" => instance.set_status_init_failed(e),
            _ => instance.set_status_init_failed(json_encode(e))
        }
        throw;
    }
    ctx = import_run("init_from_post", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    context
}
