fn workload(list, duration) {
    for wl in list.filter(|wl| ["Deployment", "DaemonSet", "StatefulSet"].contains(wl.kind)) {
        log_info(`Waiting for ${wl.kind} ${wl.namespace} ${wl.name} to be available`);
        if wl.kind == "Deployment" {
            let deploy = get_deployment(wl.namespace, w.name);
            deploy.wait_available(duration);
        } else if wl.kind == "DaemonSet" {
            let ds = get_deamonset(wl.namespace, wl.name);
            ds.wait_available(duration);
        } else if wl.kind == "StatefulSet" {
            let sts = get_statefulset(wl.namespace, wl.name);
            sts.wait_available(duration);
        }
    }
}
fn workload(list) {
    workload(list, 2*60);
}

fn job(list, duration) {
    for j in applied_objs.filter(|j| j.kind == "Job") {
        log_info(`Waiting for ${j.kind} ${j.namespace} ${j.name} to finish`);
        let job = get_job(j.namespace, j.name);
        job.wait_done(duration);
    }
}
fn job(list) {
    job(list, 5*60);
}

fn vital(list, duration) {
    for v in list {
        if ["Cluster"].contains(v.kind) {
            log_info(`Waiting for ${v.kind} ${v.namespace} ${v.name} to be available`);
            let api = k8s_resource(v.kind, v.namespace);
            let obj = api.get_obj(v.name);
            obj.wait_condition("Ready", duration);
        } else if ["RabbitmqCluster"].contains(v.kind) {
            log_info(`Waiting for ${v.kind} ${v.namespace} ${v.name} to be available`);
            let api = k8s_resource(v.kind, v.namespace);
            let obj = api.get_obj(v.name);
            obj.wait_condition("ClusterAvailable", duration);
        } else if ["Redis", "MongoDBCommunity"].contains(v.kind) {
            log_info(`Waiting for ${v.kind} ${v.namespace} ${v.name} to be available`);
            let api = k8s_resource(v.kind, v.namespace);
            let sts = get_statefulset(v.namespace, v.name);
            sts.wait_available(duration);
        }
    }
}
fn vital(list) {
    vital(list, 2*60);
}

fn all(list, duration) {
    vital(list, duration);
    job(list, duration);
    workload(list, duration);
}
fn all(list) {
    vital(list);
    job(list);
    workload(list);
}
