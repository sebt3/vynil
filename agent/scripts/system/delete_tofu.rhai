import "tofu" as tf;
import "tofu_gen" as tfg;

fn run(instance, context) {
    import_run("delete_tofu_pre", instance, context);
    let orig = instance.get_tfstate();
    if orig != "" {
        file_write(`${context.package_dir}/tofu/terraform.tfstate`, orig);
    }
    tfg::gen_files(context);
    tf::run_init(`${context.package_dir}/tofu`);
    tf::run_destroy(`${context.package_dir}/tofu`);
    import_run("delete_tofu_post", instance, context);
    context
}
