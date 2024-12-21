fn run(instance, context) {
    let ctx = import_run("delete_pre", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    if is_dir(`${context.package_dir}/scalables`) {
        ctx = import_run("delete_scalables", instance, context);
        if type_of(ctx) == "map" {
            context = ctx;
        }
    }
    if is_dir(`${context.package_dir}/tofu`) {
        context = import_run("delete_tofu", instance, context);
    }
    if is_dir(`${context.package_dir}/others`) {
        ctx = import_run("delete_others", instance, context);
        if type_of(ctx) == "map" {
            context = ctx;
        }
    }
    if is_dir(`${context.package_dir}/vitals`) {
        ctx = import_run("delete_vitals", instance, context);
        if type_of(ctx) == "map" {
            context = ctx;
        }
    }
    import_run("delete_post", instance, context);
}
