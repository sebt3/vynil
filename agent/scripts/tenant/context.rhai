import "build_context" as build;
fn run(instance, args) {
    let context = build::run(instance, args);
    let name = instance.get_tenant_name();
    let nss = instance.get_tenant_namespaces();
    context["tenant"] = #{
        name: name,
        namespaces: nss,
    };
    context = import_run("context_tenant", instance, context);
    let extra = import_run("context_extra", instance, context);
    context["extra"] = extra;
    context
}