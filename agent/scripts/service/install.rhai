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
        instance = get_service_instance(instance.metadata.namespace, instance.metadata.name);
    }
    if is_dir(`${context.package_dir}/befores`) {
        ctx = import_run("install_befores", instance, context);
        if type_of(ctx) == "map" {
            context = ctx;
        }
        instance = get_service_instance(instance.metadata.namespace, instance.metadata.name);
    }
    if is_dir(`${context.package_dir}/vitals`) {
        ctx = import_run("install_vitals", instance, context);
        if type_of(ctx) == "map" {
            context = ctx;
        }
        instance = get_service_instance(instance.metadata.namespace, instance.metadata.name);
    }
    if is_dir(`${context.package_dir}/tofu`) {
        context = import_run("install_tofu", instance, context);
        if type_of(ctx) == "map" {
            context = ctx;
        }
        instance = get_service_instance(instance.metadata.namespace, instance.metadata.name);
    }
    // there is some vitals, currently running initial installation and an initFrom is set
    if instance.status != () && instance.status.vitals != () && type_of(instance.status.tag) == "()" && instance.spec.initFrom != () {
        import_run("init_from", instance, context);
    }
    if is_dir(`${context.package_dir}/others`) {
        ctx = import_run("install_others", instance, context);
        if type_of(ctx) == "map" {
            context = ctx;
        }
        instance = get_service_instance(instance.metadata.namespace, instance.metadata.name);
    }
    if is_dir(`${context.package_dir}/scalables`) {
        ctx = import_run("install_scalables", instance, context);
        if type_of(ctx) == "map" {
            context = ctx;
        }
        instance = get_service_instance(instance.metadata.namespace, instance.metadata.name);
    }
    if is_dir(`${context.package_dir}/posts`) {
        ctx = import_run("install_posts", instance, context);
        if type_of(ctx) == "map" {
            context = ctx;
        }
        instance = get_service_instance(instance.metadata.namespace, instance.metadata.name);
    }
    // there is some vitals to backup and backup flag is set
    if instance.status != () && instance.status.vitals != () && context.namespace.use_backup {
        let found = false;
        try {
            let settings = k8s_resource("Secrets", context.instance.namespace).get("backup-settings");
            found = true;
        } catch {}
        if found {
            // this need the scalables objects to be set
            import_run("schedule_backup", instance, context);
        } else {
            import_run("delete_backup", instance, context);
        }
    } else {
        import_run("delete_backup", instance, context);
    }
    ctx = import_run("install_post", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    instance.set_status_ready(context.instance.requested);
}
