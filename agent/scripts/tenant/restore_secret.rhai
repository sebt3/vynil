fn run(context) {
    import_run("restore_secret_pre", context);
    let api = k8s_resource("Secrets", context.namespace);
    let hbs = new_hbs();
    hbs.register_partial_dir(context.template_dir);
    for secret in context.secret_list {
        let name = get_env(`secret_${secret}_target`);
        log_info(`Restoring secret ${secret} (${name})`);
        let data = #{};
        for file in read_dir(`/backup/${secret}`) {
            let base = basename(file);
            data[base] = file_read(file);
        }
        api.apply(name, #{
            apiVersion: "v1",
            kind: "Secret",
            metadata: #{
                name: name
            },
            stringData: data
        });
    }
    import_run("restore_secret_post", context);
}
