import "build_context" as build;
fn run(instance, args) {
    let context = build::run(instance, args);
    let extra = import_run("context_extra", instance, context);
    context["extra"] = extra;
    context
}