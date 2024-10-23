context["job_name"] = `scan-${box.metadata.name}`;

let cronjob_api = k8s_resource("CronJob", context.vynil_namespace);
try {
    cronjob_api.delete(context.job_name);
} catch (e) {}

let job_api = k8s_resource("Job", context.vynil_namespace);
try {
    job_api.delete(context.job_name);
} catch (e) {}
