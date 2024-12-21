fn run(instance, context) {
    let ctx = import_run("install_pre", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    if is_dir(`${context.package_dir}/vitals`) {
        ctx = import_run("install_vitals", instance, context);
        if type_of(ctx) == "map" {
            context = ctx;
        }
        instance = get_tenant_instance(instance.metadata.namespace, instance.metadata.name);
    }
    if is_dir(`${context.package_dir}/tofu`) {
        context = import_run("install_tofu", instance, context);
        if type_of(ctx) == "map" {
            context = ctx;
        }
        instance = get_tenant_instance(instance.metadata.namespace, instance.metadata.name);
    }
    if is_dir(`${context.package_dir}/others`) {
        ctx = import_run("install_others", instance, context);
        if type_of(ctx) == "map" {
            context = ctx;
        }
        instance = get_tenant_instance(instance.metadata.namespace, instance.metadata.name);
    }
    if is_dir(`${context.package_dir}/scalables`) {
        ctx = import_run("install_scalables", instance, context);
        if type_of(ctx) == "map" {
            context = ctx;
        }
        instance = get_tenant_instance(instance.metadata.namespace, instance.metadata.name);
    }
    ctx = import_run("install_post", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    instance.set_status_ready(context.instance.requested);
}
