fn run(context) {
    import_run("maintenance_stop_pre", context);
    // TODO: scale "scalables" (aka : context.deployment_list and context.statefulset_list) to context.scale_target
    import_run("maintenance_stop_post", context);
}
