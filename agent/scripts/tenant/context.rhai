import "build_context" as build;
fn run(instance, args) {
    let context = build::run(instance, args);
    import_run("context_pre", instance, context);
    let name = instance.get_tenant_name();
    let nss = instance.get_tenant_namespaces();
    context["tenant"] = #{
        name: name,
        namespaces: nss,
    };
    let ctx = import_run("context_tenant", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    let extra = import_run("context_extra", instance, context);
    context["extra"] = extra;
    import_run("context_post", instance, context);
    context
}