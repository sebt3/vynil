fn run(context) {
    import_run("restore_run_pre", context);
    log_info(`Starting restic restore for ${context.snapshot}`);
    let rc = shell_run(`export RESTIC_REPOSITORY="${context.s3_url}";restic restore ${context.snapshot} --target / ${context.restic_args}`);
    if rc != 0 {
        throw `restic backup FAILED returning ${rc}`;
    }
    import_run("restore_run_post", context);
}
