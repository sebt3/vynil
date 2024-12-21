fn run(instance, context) {
    let ctx = import_run("delete_pre", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    if is_dir(`${context.package_dir}/systems`) {
        ctx = import_run("delete_systems", instance, context);
        if type_of(ctx) == "map" {
            context = ctx;
        }
    }
    if is_dir(`${context.package_dir}/tofu`) {
        context = import_run("delete_tofu", instance, context);
    }
    if is_dir(`${context.package_dir}/crds`) {
        ctx = import_run("delete_crds", instance, context);
        if type_of(ctx) == "map" {
            context = ctx;
        }
    }
    import_run("delete_post", instance, context);
}
