fn run(context) {
    import_run("backup_prepare_mongodb_pre", context);
    for mongo in context.mongo_list {
        log_info(`Dumping mongodb target: ${mongo}`);
        let dbname = get_env(`${mongo}_dbname`);
        let base_args = "--host=\"${"+mongo+"_host}\" --username=\"${"+mongo+"_username}\" --password=\"${"+mongo+"_password}\" --authenticationDatabase=admin";
        let rc = if dbname == "*" {
            let rc_oplog = shell_run("mongodump "+base_args+" --oplog --archive=/backup/mongodb_"+mongo+".archive");
            if rc_oplog != 0 {
                log_warn(`mongodump --oplog failed for ${mongo}, retrying without (backup role may be missing)`);
                shell_run("mongodump "+base_args+" --numParallelCollections=1 --archive=/backup/mongodb_"+mongo+".archive")
            } else {
                rc_oplog
            }
        } else {
            shell_run("mongodump "+base_args+" --numParallelCollections=1 --db=\"${"+mongo+"_dbname}\" --archive=/backup/mongodb_"+mongo+".archive")
        };
        if rc != 0 {
            throw `mongodump failed for ${mongo}`;
        }
    }
    import_run("backup_prepare_mongodb_post", context);
}
