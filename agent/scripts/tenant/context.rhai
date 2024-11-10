import "build_context" as build;
fn run(instance, args) {
    let context = build::run(instance, args);
    let name = instance.get_tenant_name();
    let nss = instance.get_tenant_namespaces();
    //TODO: complete tenant values (ha, backup, sso, mail)
    context["tenant"] = #{
        name: name,
        namespaces: nss,
    };
    //TODO: namespace config
    let extra = import_run("context_extra", instance, context);
    context["extra"] = extra;
    context
}