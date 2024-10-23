fn run(instance, context) {
    import_run("install_pre", instance, context);
    if is_dir(`${context.package_dir}/tofu`) {
        context = import_run("install_tofu", instance, context);
        instance = get_tenant_instance(instance.metadata.namespace, instance.metadata.name);
    }
    if is_dir(`${context.package_dir}/vitals`) {
        import_run("install_vitals", instance, context);
        instance = get_tenant_instance(instance.metadata.namespace, instance.metadata.name);
    }
    if is_dir(`${context.package_dir}/others`) {
        import_run("install_others", instance, context);
        instance = get_tenant_instance(instance.metadata.namespace, instance.metadata.name);
    }
    if is_dir(`${context.package_dir}/scalables`) {
        import_run("install_scalables", instance, context);
        instance = get_tenant_instance(instance.metadata.namespace, instance.metadata.name);
    }
    import_run("install_post", instance, context);
    instance.set_status_ready(context.instance.requested);
}
