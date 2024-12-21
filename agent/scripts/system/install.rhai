fn run(instance, context) {
    let ctx = import_run("install_pre", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
   if is_dir(`${context.package_dir}/crds`) {
        log_info(`Installing CRDs from ${context.package_dir}/crds from ${instance.metadata.namespace} ${instance.metadata.name}`);
        ctx = import_run("install_crds", instance, context);
        if type_of(ctx) == "map" {
            context = ctx;
        }
        update_k8s_crd_cache();
        instance = get_system_instance(instance.metadata.namespace, instance.metadata.name);
    }
    if is_dir(`${context.package_dir}/tofu`) {
        log_info(`Running tofu apply from ${context.package_dir}/tofu from ${instance.metadata.namespace} ${instance.metadata.name}`);
        context = import_run("install_tofu", instance, context);
        instance = get_system_instance(instance.metadata.namespace, instance.metadata.name);
    }
    if is_dir(`${context.package_dir}/systems`) {
        log_info(`Installing systems from ${context.package_dir}/systems from ${instance.metadata.namespace} ${instance.metadata.name}`);
        ctx = import_run("install_systems", instance, context);
        if type_of(ctx) == "map" {
            context = ctx;
        }
        instance = get_system_instance(instance.metadata.namespace, instance.metadata.name);
    }
    import_run("install_post", instance, context);
    instance.set_status_ready(context.instance.requested);
}
