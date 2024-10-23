context["name"] = instance.metadata.name;
context["namespace"] = instance.metadata.namespace;
context["package_type"] = "tenant";
context["package_action"] = "delete";
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
}
if ! failed {
    // all green to start agent
    let job_def = yaml_decode(hbs.render_from("{{> package.yaml }}", context));
    let job_api = k8s_resource("Job", context.vynil_namespace);
    try {
        job_api.delete(context.job_name);
    } catch (e) {}
    job_api.apply(context.job_name, job_def);
    let job = get_job(context.vynil_namespace, context.job_name);
    job.wait_done(10*60);
}
failed