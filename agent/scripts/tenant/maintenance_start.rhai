fn run(context) {
    import_run("maintenance_start_pre", context);
    // TODO: scale "scalables" (aka : context.deployment_list and context.statefulset_list) to 0
    import_run("maintenance_start_post", context);
}
