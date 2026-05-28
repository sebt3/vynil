fn run(context) {
    import_run("backup_prepare_redis_pre", context);
    for redis in context.redis_list {
        log_info(`Dumping redis target: ${redis}`);
        let rc = shell_run("redis-cli -h \"${"+redis+"_host}\" --pass \"${"+redis+"_password}\" --no-auth-warning --rdb /backup/redis_"+redis+".rdb");
        if rc != 0 {
            throw `redis-cli rdb dump failed for ${redis}`;
        }
    }
    import_run("backup_prepare_redis_post", context);
}
