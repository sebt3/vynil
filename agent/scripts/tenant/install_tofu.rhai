import "tofu" as tf;
import "tofu_gen" as tfg;
fn run(instance, context) {
    let ctx = import_run("install_tofu_pre", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    let orig = instance.get_tfstate();
    if orig != "" {
        file_write(`${context.package_dir}/tofu/terraform.tfstate`, orig);
    }
    try {
        tfg::gen_files(context);
        tf::run_init(`${context.package_dir}/tofu`);
        tf::run_plan(`${context.package_dir}/tofu`);
        tf::run_apply(`${context.package_dir}/tofu`);
        context.tofu = json_decode(tf::get_output(`${context.package_dir}/tofu`));
        let state = file_read(`${context.package_dir}/tofu/terraform.tfstate`);
        instance.set_tfstate(state);
    } catch (e) {
        let state = if (is_file(`${context.package_dir}/tofu/terraform.tfstate`)) {
            file_read(`${context.package_dir}/tofu/terraform.tfstate`)
        } else {""};
        switch type_of(e) {
            "string" => instance.set_status_tofu_failed(state, e),
            _ => instance.set_status_tofu_failed(state, json_encode(e))
        }
        throw e;
    }
    ctx = import_run("install_tofu_post", instance, context);
    if type_of(ctx) == "map" {
        context = ctx;
    }
    context
}
