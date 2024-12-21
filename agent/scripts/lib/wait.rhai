fn workload(lst, duration) {
    for wl in lst.filter(|wl| ["Deployment", "DaemonSet", "StatefulSet"].contains(wl.kind)) {
        log_info(`Waiting for ${wl.kind} ${wl.namespace}/${wl.name} to be available`);
        if wl.kind == "Deployment" {
            let deploy = get_deployment(wl.namespace, wl.name);
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
fn workload(lst) {
    workload(lst, 2*60);
}

fn job(lst, duration) {
    for j in lst.filter(|j| j.kind == "Job") {
        log_info(`Waiting for ${j.kind} ${j.namespace}/${j.name} to finish`);
        let job = get_job(j.namespace, j.name);
        job.wait_done(duration);
    }
}
fn job(lst) {
    job(lst, 5*60);
}

fn vital(lst, duration) {
    for v in lst {
        if ["Cluster"].contains(v.kind) {
            log_info(`Waiting for ${v.kind} ${v.namespace}/${v.name} to be available`);
            let api = k8s_resource(v.kind, v.namespace);
            let obj = api.get_obj(v.name);
            obj.wait_condition("Ready", duration);
        } else if ["RabbitmqCluster"].contains(v.kind) {
            log_info(`Waiting for ${v.kind} ${v.namespace}/${v.name} to be available`);
            let api = k8s_resource(v.kind, v.namespace);
            let obj = api.get_obj(v.name);
            obj.wait_condition("ClusterAvailable", duration);
        } else if ["RedisSentinel"].contains(v.kind) {
            log_info(`Waiting for ${v.kind} ${v.namespace}/${v.name} to be available`);
            let sts = get_statefulset(v.namespace, `${v.name}-sentinel`);
            sts.wait_available(duration);
        } else if ["RedisCluster"].contains(v.kind) {
            log_info(`Waiting for ${v.kind} ${v.namespace}/${v.name} to be available`);
            let sts = get_statefulset(v.namespace, `${v.name}-leader`);
            sts.wait_available(duration);
        } else if ["Redis", "RedisReplication", "MongoDBCommunity"].contains(v.kind) {
            log_info(`Waiting for ${v.kind} ${v.namespace}/${v.name} to be available`);
            let sts = get_statefulset(v.namespace, v.name);
            sts.wait_available(duration);
        }
    }
}
fn vital(lst) {
    vital(lst, 2*60);
}

fn all(lst, duration) {
    vital(lst, duration);
    job(lst, duration);
    workload(lst, duration);
}
fn all(lst) {
    vital(lst);
    job(lst);
    workload(lst);
}
