fn run(context) {
    import_run("backup_prepare_secret_pre", context);
    for secret in context.secret_list {
        log_info(`Saving secret: ${secret}`);
        create_dir(`/backup/${secret}`);
        for file in read_dir(`/secrets/${secret}`) {
            let base = basename(file);
            if ! base.starts_with(".") {
                file_copy(file, `/backup/${secret}/${base}`);
            }
        }
    }
    import_run("backup_prepare_secret_post", context);
}
