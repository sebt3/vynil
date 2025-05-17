import "install_from_dir" as dir;
fn run(instance, context) {
    let ctx = import_template("install_systems_pre", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    dir::template(instance, context, `${context.package_dir}/systems`);
    context
}
