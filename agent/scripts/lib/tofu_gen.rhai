import "tofu" as tf;
fn gen_files(context) {
    gen_provider(`${context.package_dir}/tofu`);
    gen_locals(context);
    gen_vars(context.instance["package"].options, `${context.package_dir}/tofu`);
    gen_tfvars(context);
}

fn gen_vars(options, path) {
    let str = "";
    if options != () {
        for k in options.keys() {
            let t = tf::get_tf_type(options[k], false);
            let d = tf::to_tf(options[k]["default"]);
            str += `variable "${k}" {
  default = ${d}  type    = ${t}
}

`;
        }
    }
    file_write(`${path}/00_vynil_vars.tf`, str);
}

fn gen_tfvars(context) {
    let str = "";
    if context.values != () {
        for k in context.values.keys() {
            let v = tf::to_tf(context.values[k]);
            str+=`${k} = ${v}
`;
        }
    }
    file_write(`${context.package_dir}/tofu/vynil.tfvars`, str);
}

fn gen_locals(context) {
    let cluster = tf::to_tf(context.cluster);
    let instance = tf::to_tf(context.instance);
    let defaults = tf::to_tf(context.defaults);
    let extra = tf::to_tf(context.extra);
    let owner = tf::to_tf(vynil_owner());
    file_write(`${context.package_dir}/tofu/00_vynil_locals.tf`, `locals {
    vynil_cluster = ${cluster}
    vynil_instance = ${instance}
    vynil_defaults = ${defaults}
    vynil_extra = ${extra}
    vynil_owner = ${owner}
}`);
}

fn gen_provider(path) {
    if ! is_file(`${path}/providers.tf`) {
        file_write(`${path}/providers.tf`, "terraform {\n\
  required_providers {\n\
    kubernetes = {\n\
        source = \"hashicorp/kubernetes\"\n\
        version = \"~> 2.20.0\"\n\
    }\n\
    kubectl = {\n\
        source = \"gavinbunney/kubectl\"\n\
        version = \"~> 1.14.0\"\n\
    }\n\
  }\n\
}\n\
provider \"kubernetes\" {\n\
    host = \"https://kubernetes.default.svc\"\n\
    token = \"${file(\"/run/secrets/kubernetes.io/serviceaccount/token\")}\"\n\
    cluster_ca_certificate = \"${file(\"/run/secrets/kubernetes.io/serviceaccount/ca.crt\")}\"\n\
}\n\
provider \"kubectl\" {\n\
    host = \"https://kubernetes.default.svc\"\n\
    token = \"${file(\"/run/secrets/kubernetes.io/serviceaccount/token\")}\"\n\
    cluster_ca_certificate = \"${file(\"/run/secrets/kubernetes.io/serviceaccount/ca.crt\")}\"\n\
    load_config_file       = false\n\
}");
    }
}
