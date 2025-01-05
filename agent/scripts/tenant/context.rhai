import "build_context" as build;
fn run(instance, args) {
    let context = build::run(instance, args);
    context["template_dir"] = args.template_dir;
    context["agent_image"] = args.agent_image;
    let ctx = import_run("context_pre", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    let name = instance.get_tenant_name();
    let nss = instance.get_tenant_namespaces();
    context["tenant"] = #{
        name: name,
        namespaces: nss,
        maintenance_start_hour: "1",
        maintenance_start_minut: "0",
    };
    ctx = import_run("context_tenant", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    context["extra"] = import_run("context_extra", instance, context);
    ctx = import_run("context_post", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    context
}