fn run(context) {
    import_run("restore_redis_pre", context);
    let ns = context.namespace;
    for redis in context.redis_list {
        log_info(`Starting Redis restore for ${redis}`);
        let sts  = get_env(`${redis}_sts`);
        let pass = get_env(`${redis}_password`);

        // Detect whether the workload is a StatefulSet or a Deployment
        let is_sts = shell_run(`kubectl -n ${ns} get statefulset ${sts} 2>/dev/null`) == 0;
        let resource = if is_sts { "statefulset" } else { "deployment" };

        // Scale down to 1 replica to avoid cross-replica corruption during restore
        let replicas = parse_int(shell_output(
            `kubectl -n ${ns} get ${resource} ${sts} -o jsonpath='{.spec.replicas}'`));
        if replicas > 1 {
            let rc = shell_run(`kubectl -n ${ns} scale ${resource}/${sts} --replicas=1`);
            if rc != 0 { throw `scale down failed for ${redis}`; }
            let rc = shell_run(`kubectl -n ${ns} rollout status --timeout=5m ${resource}/${sts}`);
            if rc != 0 { throw `scale down wait failed for ${redis}`; }
        }

        let pod = shell_output(`kubectl -n ${ns} get pods -l app=${sts} -o jsonpath='{.items[0].metadata.name}'`);
        if pod == "" {
            throw `No pod found for redis ${redis} (${resource}: ${sts})`;
        }

        // Disable auto-save on the running pod so it cannot overwrite our dump on shutdown.
        // CONFIG SET is runtime-only: the new pod re-reads its static config on startup,
        // so auto-save is automatically re-enabled after the rollout restart.
        shell_run(`kubectl -n ${ns} exec ${pod} -- redis-cli --no-auth-warning -a "${pass}" CONFIG SET save ""`);

        let rc = shell_run(`kubectl -n ${ns} cp /backup/redis_${redis}.rdb ${pod}:/data/dump.rdb`);
        if rc != 0 {
            throw `kubectl cp failed for ${redis}`;
        }

        let rc = shell_run(`kubectl -n ${ns} rollout restart ${resource}/${sts}`);
        if rc != 0 {
            throw `rollout restart failed for ${redis}`;
        }
        let rc = shell_run(`kubectl -n ${ns} rollout status --timeout=5m ${resource}/${sts}`);
        if rc != 0 {
            throw `rollout status failed for ${redis}`;
        }

        // Scale back to the original replica count
        if replicas > 1 {
            shell_run(`kubectl -n ${ns} scale ${resource}/${sts} --replicas=${replicas}`);
            shell_run(`kubectl -n ${ns} rollout status --timeout=5m ${resource}/${sts}`);
        }
    }
    import_run("restore_redis_post", context);
}
