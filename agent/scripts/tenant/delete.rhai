fn run(instance, context) {
    import_run("delete_pre", instance, context);
    if is_dir(`${context.package_dir}/tofu`) {
        context = import_run("delete_tofu", instance, context);
    }
    if is_dir(`${context.package_dir}/scalables`) {
        import_run("delete_scalables", instance, context);
    }
    if is_dir(`${context.package_dir}/others`) {
        import_run("delete_others", instance, context);
    }
    if is_dir(`${context.package_dir}/vitals`) {
        import_run("delete_vitals", instance, context);
    }
    import_run("delete_post", instance, context);
}
