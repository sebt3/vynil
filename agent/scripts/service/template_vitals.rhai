import "install_from_dir" as dir;
import "wait" as wait;
fn run(instance, context) {
    let ctx = import_template("install_vitals_pre", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    dir::template(instance, context, `${context.package_dir}/vitals`);
    ctx = import_template("install_vitals_post", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    context
}
