fn run(context) {
    import_run("backup_prepare_mongodb_pre", context);
    for mongo in context.mongo_list {
        log_info(`Dumping mongodb target: ${mongo}`);
        let dbname = get_env(`${mongo}_dbname`);
        let rc = if dbname == "*" {
            shell_run("mongodump --host=\"${"+mongo+"_host}\" --username=\"${"+mongo+"_username}\" --password=\"${"+mongo+"_password}\" --authenticationDatabase=admin --archive=/backup/mongodb_"+mongo+".archive")
        } else {
            shell_run("mongodump --host=\"${"+mongo+"_host}\" --username=\"${"+mongo+"_username}\" --password=\"${"+mongo+"_password}\" --authenticationDatabase=admin --db=\"${"+mongo+"_dbname}\" --archive=/backup/mongodb_"+mongo+".archive")
        };
        if rc != 0 {
            throw `mongodump failed for ${mongo}`;
        }
    }
    import_run("backup_prepare_mongodb_post", context);
}
