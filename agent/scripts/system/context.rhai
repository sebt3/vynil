import "build_context" as build;
fn template(instance, args) {
    let context = build::template(instance, args);
    let ctx = import_template("context_pre", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    ctx = import_template("context_system", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    let extra = import_template("context_extra", instance, context);
    context["extra"] = extra;
    ctx = import_template("context_post", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    context
}

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
    ctx = import_run("context_after", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    context
}