context["name"] = box.metadata.name;
context["job_name"] = `scan-${box.metadata.name}`;
context["schedule"] = box.spec.schedule;
let cj_def = yaml_decode(hbs.render_from("{{> cronscan.yaml }}", context));
let cronjob_api = k8s_resource("CronJob", context.vynil_namespace);
cronjob_api.apply(context.job_name, cj_def);

let job_def = yaml_decode(hbs.render_from("{{> scan.yaml }}", context));
let job_api = k8s_resource("Job", context.vynil_namespace);
try {
    log_info("Create Job");
    job_api.apply(context.job_name, job_def);
} catch (e) {
    debug(e);
    let j = job_api.get_obj(context.job_name);
    j.delete();
    j.wait_deleted(60);
    job_api.create(job_def);
}
let job = get_job(context.vynil_namespace, context.job_name);
job.wait_done(5*60);
