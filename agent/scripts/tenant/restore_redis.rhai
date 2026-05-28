fn run(context) {
    import_run("restore_redis_pre", context);
    let ns = context.namespace;
    for redis in context.redis_list {
        log_info(`Starting Redis restore for ${redis}`);
        let kind = get_env(`${redis}_kind`);
        let sts  = get_env(`${redis}_sts`);
        let pod  = shell_output(`kubectl -n ${ns} get pods -l app=${sts} -o jsonpath='{.items[0].metadata.name}'`);
        if pod == "" {
            throw `No pod found for redis ${redis} (statefulset: ${sts})`;
        }
        let rc = shell_run(`kubectl -n ${ns} cp /backup/redis_${redis}.rdb ${pod}:/data/dump.rdb`);
        if rc != 0 {
            throw `kubectl cp failed for ${redis}`;
        }
        let rc2 = if kind == "Dragonfly" {
            shell_run(`kubectl -n ${ns} rollout restart deployment/${sts}`)
        } else {
            shell_run(`kubectl -n ${ns} rollout restart statefulset/${sts}`)
        };
        if rc2 != 0 {
            throw `rollout restart failed for ${redis}`;
        }
        let rc3 = shell_run(`kubectl -n ${ns} rollout status --timeout=5m statefulset/${sts}`);
        if rc3 != 0 {
            throw `rollout status check failed for ${redis}`;
        }
    }
    import_run("restore_redis_post", context);
}
