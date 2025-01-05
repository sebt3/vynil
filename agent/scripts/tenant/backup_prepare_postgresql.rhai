fn run(context) {
    import_run("backup_prepare_postgresql_pre", context);
    for pg in context.pg_list {
        log_info(`Dumping postgresql target: ${pg}`);
        let dbname = get_env(`${pg}_dbname`);
        let rc = if dbname == "*" {
            shell_run("export PGPASSWORD=\"${"+pg+"_password}\";pg_dumpall -h \"${"+pg+"_host}\" -U \"${"+pg+"_username}\" --clean -f /backup/postgresql_"+pg+".sql")
        } else {
            shell_run("export PGPASSWORD=\"${"+pg+"_password}\";pg_dump -h \"${"+pg+"_host}\" -d \"${"+pg+"_dbname}\" -U \"${"+pg+"_username}\" --clean -f /backup/postgresql_"+pg+".sql")
        };
        if rc != 0 {
            throw `pg_dump failed for ${pg}`;
        }
    }
    import_run("backup_prepare_postgresql_post", context);
}
