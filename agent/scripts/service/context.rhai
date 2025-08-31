import "build_context" as build;
fn template(instance, args) {
    let context = build::template(instance, args);
    context["template_dir"] = args.template_dir;
    context["agent_image"] = args.agent_image;
    let ctx = import_template("context_pre", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    context["namespace"] = #{
        maintenance_start_hour: "0",
        maintenance_start_minut: "0",
    };
    ctx = import_template("context_service", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    context["extra"] = import_template("context_extra", instance, context);
    ctx = import_template("context_post", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    context
}
fn run(instance, args) {
    let context = build::run(instance, args);
    context["template_dir"] = args.template_dir;
    context["agent_image"] = args.agent_image;
    let ctx = import_run("context_pre", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    context["namespace"] = #{
        maintenance_start_hour: "0",
        maintenance_start_minut: "0",
    };
    ctx = import_run("context_service", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    context["extra"] = import_run("context_extra", instance, context);
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
