fn run(context) {
    import_run("restore_mysql_pre", context);
    for mysql in context.mysql_list {
        log_info(`Starting MySQL restore for ${mysql}`);
        let dbname = get_env(`${mysql}_dbname`);
        let args = "--skip-ssl --host=\"${"+mysql+"_host}\" --user=\"${"+mysql+"_username}\" --password=\"${"+mysql+"_password}\"";
        let file = `/backup/mysql_${mysql}.sql`;
        if dbname != "*" {
            args += " --one-database \"${"+mysql+"_dbname}\"";
        }
        if is_file(`/backup/mysql_${mysql}_cleaned.sql`) {
            file = `/backup/mysql_${mysql}_cleaned.sql`;
        }
        let rc = shell_run("mysql "+args+" < "+file);
        if rc != 0 {
            throw `mysql restoration failed for ${mysql}`;
        }
    }
    import_run("restore_mysql_post", context);
}
