fn run(instance, context) {
    import_run("delete_pre", instance, context);
    if is_dir(`${context.package_dir}/systems`) {
        import_run("delete_systems", instance, context);
    }
    if is_dir(`${context.package_dir}/tofu`) {
        context = import_run("delete_tofu", instance, context);
    }
    if is_dir(`${context.package_dir}/crds`) {
        import_run("delete_crds", instance, context);
    }
    import_run("delete_post", instance, context);
}
