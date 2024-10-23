context["name"] = instance.metadata.name;
context["namespace"] = instance.metadata.namespace;
context["package_type"] = "system";
context["package_action"] = "install";
context["job_name"] = `${context.package_type}--${instance.metadata.namespace}--${instance.metadata.name}`;
context["digest"] = instance.options_digest();
let failed = false;
if ! (instance.spec.jukebox in packages) {
    failed = true;
    instance.set_missing_box(instance.spec.jukebox);
} else if ! (packages[instance.spec.jukebox].some(|p| p.metadata.type == context.package_type && p.metadata.name == instance.spec["package"] && p.metadata.category == instance.spec.category )) {
    failed = true;
    instance.set_missing_package(instance.spec.category, instance.spec["package"]);
} else {
    let pck = packages[instance.spec.jukebox].find(|p| p.metadata.type == context.package_type && p.metadata.name == instance.spec["package"] && p.metadata.category == instance.spec.category);
    context["tag"] = pck.tag;
    context["image"] = pck.image;
    context["registry"] = pck.registry;
    if "pull_secret" in pck {
        context["use_secret"] = true;
        context["pull_secret"] = pck.pull_secret;
    } else {
        context["use_secret"] = false;
    }
    try {
        for r in pck.requirements {
            if "custom_resource_definition" in r && ! failed {
                let crds = k8s_resource("CustomResourceDefinition").list_meta().items.map(|c| c.metadata.name);
                if ! (r.custom_resource_definition in crds) {
                    failed = true;
                    instance.set_missing_requirement(`CRD ${r.custom_resource_definition} is not installed`);
                }
            } else if "prefly" in r && ! failed {
                if ! eval(r.prefly.script) {
                    failed = true;
                    instance.set_missing_requirement(`Requirement ${r.prefly.name} failed`);
                }
            } else if "system_package" in r && ! failed {
                let sinstances = k8s_resource("SystemInstance").list().items;
                if ! (sinstances.some(|i| i.spec.category == r.system_package.category && i.spec["package"] == r.system_package.name)) {
                    failed = true;
                    instance.set_missing_requirement(`Package ${r.system_package.category}/${r.system_package.name} is not installed`);
                }
            } else if "tenant_package" in r {
                log_warn(`tenant_package requirements for a system package is invalid, skipping`);
            } else if "storage_capability" in r /* && ! failed */ {
                log_warn(`storage_capability requirements is a TODO`);
            } else if "minimum_previous_version" in r /* && ! failed */ {
                log_warn(`minimum_previous_version requirements is a TODO`);
            }
        }
    } catch(e) {
        failed = true;
        if type_of(e) == "string" {
            instance.set_missing_requirement(e);
        } else {
            instance.set_missing_requirement(json_encode(e));
        }
    }
}
if ! failed {
    // all green to start agent
    let job_def = yaml_decode(hbs.render_from("{{> package.yaml }}", context));
    let job_api = k8s_resource("Job", context.vynil_namespace);
    try {
        job_api.apply(context.job_name, job_def);
    } catch (e) {
        debug(e);
        let j = job_api.get_obj(context.job_name);
        j.delete();
        j.wait_deleted(60);
        instance.set_agent_started();
        job_api.create(job_def);
    }
    let job = get_job(context.vynil_namespace, context.job_name);
    job.wait_done(10*60);
}
failed