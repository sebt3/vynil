fn run(instance, context) {
    import_run("install_pre", instance, context);
    if is_dir(`${context.package_dir}/crds`) {
        log_info(`Installing CRDs from ${context.package_dir}/crds from ${instance.metadata.namespace} ${instance.metadata.name}`);
        import_run("install_crds", instance, context);
    }
    if is_dir(`${context.package_dir}/tofu`) {
        log_info(`Running tofu apply from ${context.package_dir}/tofu from ${instance.metadata.namespace} ${instance.metadata.name}`);
        context = import_run("install_tofu", instance, context);
    }
    if is_dir(`${context.package_dir}/systems`) {
        log_info(`Installing systems from ${context.package_dir}/systems from ${instance.metadata.namespace} ${instance.metadata.name}`);
        import_run("install_systems", instance, context);
    }
    import_run("install_post", instance, context);
    instance.set_status_ready(context.instance.requested);
}
