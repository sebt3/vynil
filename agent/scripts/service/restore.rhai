import "backup_context" as ctx;
fn run(args) {
    let context = ctx::from_args(args);
    import_run("restore_pre", context);
    if is_file(`${args.package_dir}/scripts/restore.sh`) {
        let rc = shell_run(`export RESTIC_REPOSITORY="${context.s3_url}";${context.package_dir}/scripts/restore.sh`);
        if rc != 0 {
            throw `${context.package_dir}/scripts/restore.sh FAILED returning ${rc}`;
        } else {
            import_run("restore_post", context);
            return rc;
        }
    }
    import_run("maintenance_start", context);
    import_run("restore_run", context);
    if is_file(`${args.package_dir}/scripts/restore_prepare.sh`) {
        let rc = shell_run(`${context.package_dir}/scripts/restore_prepare.sh`);
        if rc != 0 {
            throw `${context.package_dir}/scripts/restore_prepare.sh FAILED returning ${rc}`;
        }
    }
    if is_file(`${args.package_dir}/scripts/restore_secret.sh`) {
        let rc = shell_run(`${context.package_dir}/scripts/restore_secret.sh`);
        if rc != 0 {
            throw `${context.package_dir}/scripts/restore_secret.sh FAILED returning ${rc}`;
        }
    } else {
        import_run("restore_secret", context);
    }
    if is_file(`${args.package_dir}/scripts/restore_postgresql.sh`) {
        let rc = shell_run(`${context.package_dir}/scripts/restore_postgresql.sh`);
        if rc != 0 {
            throw `${context.package_dir}/scripts/restore_postgresql.sh FAILED returning ${rc}`;
        }
    } else {
        import_run("restore_postgresql", context);
    }
    if is_file(`${args.package_dir}/scripts/restore_mysql.sh`) {
        let rc = shell_run(`${context.package_dir}/scripts/restore_mysql.sh`);
        if rc != 0 {
            throw `${context.package_dir}/scripts/restore_mysql.sh FAILED returning ${rc}`;
        }
    } else {
        import_run("restore_mysql", context);
    }
    import_run("maintenance_stop", context);
    import_run("restore_post", context);
}
