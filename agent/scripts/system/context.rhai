import "build_context" as build;
fn run(instance, args) {
    let context = build::run(instance, args);
    let ctx = import_run("context_pre", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    ctx = import_run("context_system", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    let extra = import_run("context_extra", instance, context);
    context["extra"] = extra;
    ctx = import_run("context_post", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    context
}