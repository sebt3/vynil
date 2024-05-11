use std::path::{Path, PathBuf};
use anyhow::{Result, bail};
use k8s::yaml::{Providers, Component};
use serde::{Deserialize, Serialize};
use crate::shell;

pub fn gen_file(dest:&PathBuf, content: &String, force: bool) -> Result<()> {
    if ! Path::new(dest).is_file() || force {
        match std::fs::write(dest, content) {Ok(_) => {}, Err(e) => bail!("Error {} while generating: {}", e, dest.display()),};
    }
    Ok(())
}

pub fn run_init(src: &PathBuf) -> Result<()> {
    shell::run_log(&format!("cd {:?};terraform init", src))
}

pub fn run_plan(src: &PathBuf) -> Result<()> {
    // Check if "init" need to be run
    let mut file = PathBuf::new();
    file.push(src);
    file.push(".terraform.lock.hcl");
    if ! Path::new(&file).is_file() {
        match run_init(src) {Ok(_) => {}, Err(e) => {return Err(e)}}
    }
    let mut file = PathBuf::new();
    file.push(src);
    file.push("env.tfvars");
    if ! Path::new(&file).is_file() {
        bail!("`env.tfvars` should be there");
    }
    shell::run_log(&format!("cd {:?};terraform plan -input=false -out=tf.plan -var-file=env.tfvars", src))
}

pub fn run_apply(src: &PathBuf) -> Result<()> {
    let mut file = PathBuf::new();
    file.push(src);
    file.push("tf.plan");
    if ! Path::new(&file).is_file() {
        match run_plan(src) {Ok(_) => {}, Err(e) => {return Err(e)}}
    }
    shell::run_log(&format!("cd {:?};terraform apply -input=false -auto-approve tf.plan", src))
}

pub fn get_plan(src: &PathBuf) -> Result<serde_json::Map<String, serde_json::Value>> {
    let mut file = PathBuf::new();
    file.push(src);
    file.push("tf.plan");
    if ! Path::new(&file).is_file() {
        match run_plan(src) {Ok(_) => {}, Err(e) => {return Err(e)}}
    }
    let output = match shell::get_output(&format!("cd {:?};terraform show -json tf.plan", src)) {Ok(d) => d, Err(e) => {bail!("{e}")}};
    let json: serde_json::Map<String, serde_json::Value> = serde_json::from_str(output.as_str()).unwrap();
    Ok(json)
}

pub fn run_destroy(src: &PathBuf) -> Result<()> {
    // Check if "init" need to be run
    let mut file = PathBuf::new();
    file.push(src);
    file.push(".terraform.lock.hcl");
    if ! Path::new(&file).is_file() {
        match run_init(src) {Ok(_) => {}, Err(e) => {return Err(e)}}
    }
    let mut file = PathBuf::new();
    file.push(src);
    file.push("env.tfvars");
    if ! Path::new(&file).is_file() {
        bail!("`env.tfvars` should be there");
    }
    shell::run_log(&format!("cd {:?};terraform apply -destroy -input=false -auto-approve -var-file=env.tfvars", src))
}

pub fn gen_providers(dest_dir: &PathBuf, providers: Option<Providers>) -> Result<()> {
    let mut file  = PathBuf::new();
    let mut requiered = String::new();
    let mut content = String::new();
    requiered += "
terraform {
  required_providers {
    kustomization = {
        source  = \"kbst/kustomization\"
        version = \"~> 0.9.2\"
    }";
    content += "
  }
}
provider \"kustomization\" {
    kubeconfig_incluster = true
}";
    let mut have_kubernetes = false;
    if let Some(providers) = providers.clone() {
      if let Some(kubernetes) = providers.kubernetes {
        have_kubernetes = kubernetes;
      }
    }
    if have_kubernetes {
      requiered += "
    kubernetes = {
        source = \"hashicorp/kubernetes\"
        version = \"~> 2.20.0\"
    }";
      content += "
provider \"kubernetes\" {
    host = \"https://kubernetes.default.svc\"
    token = \"${file(\"/run/secrets/kubernetes.io/serviceaccount/token\")}\"
    cluster_ca_certificate = \"${file(\"/run/secrets/kubernetes.io/serviceaccount/ca.crt\")}\"
}";
    } else {
      requiered += "
#    kubernetes = {
#        source = \"hashicorp/kubernetes\"
#        version = \"~> 2.20.0\"
#    }";
      content += "
#provider \"kubernetes\" {
#    host = \"https://kubernetes.default.svc\"
#    token = \"${file(\"/run/secrets/kubernetes.io/serviceaccount/token\")}\"
#    cluster_ca_certificate = \"${file(\"/run/secrets/kubernetes.io/serviceaccount/ca.crt\")}\"
#}";
    }
    let mut have_kubectl = false;
    if let Some(providers) = providers.clone() {
      if let Some(kubectl) = providers.kubectl {
        have_kubectl = kubectl;
      }
    }
    if have_kubectl {
      requiered += "
    kubectl = {
        source = \"gavinbunney/kubectl\"
        version = \"~> 1.14.0\"
    }";
      content += "
provider \"kubectl\" {
    host = \"https://kubernetes.default.svc\"
    token = \"${file(\"/run/secrets/kubernetes.io/serviceaccount/token\")}\"
    cluster_ca_certificate = \"${file(\"/run/secrets/kubernetes.io/serviceaccount/ca.crt\")}\"
    load_config_file       = false
}";
    } else {
      requiered += "
#    kubectl = {
#        source = \"gavinbunney/kubectl\"
#        version = \"~> 1.14.0\"
#    }";
      content += "
#provider \"kubectl\" {
#    host = \"https://kubernetes.default.svc\"
#    token = \"${file(\"/run/secrets/kubernetes.io/serviceaccount/token\")}\"
#    cluster_ca_certificate = \"${file(\"/run/secrets/kubernetes.io/serviceaccount/ca.crt\")}\"
#    load_config_file       = false
#}";
    }
    let mut have_authentik = false;
    if let Some(providers) = providers.clone() {
      if let Some(authentik) = providers.authentik {
        have_authentik = authentik;
      }
    }
    if have_authentik {
      requiered += "
    authentik = {
        source = \"goauthentik/authentik\"
        version = \"~> 2023.5.0\"
    }";
      content += "
provider \"authentik\" {
  url   = local.authentik_url
  token = local.authentik_token
}";
    } else {
      requiered += "
#    authentik = {
#        source = \"goauthentik/authentik\"
#        version = \"~> 2023.5.0\"
#    }";
      content += "
#provider \"authentik\" {
#  url   = local.authentik_url
#  token = local.authentik_token
#}";
    }
    let mut have_postgresql = false;
    if let Some(providers) = providers.clone() {
      if let Some(postgresql) = providers.postgresql {
        have_postgresql = postgresql;
      }
    }
    if have_postgresql {
      requiered += "
    postgresql = {
        source = \"cyrilgdn/postgresql\"
        version = \"~> 1.19.0\"
    }";
      content += "
provider \"postgresql\" {
  host            = local.pg_host
  username        = local.pg_username
  password        = local.pg_password
}";
    } else {
      requiered += "
#    postgresql = {
#        source = \"cyrilgdn/postgresql\"
#        version = \"~> 1.19.0\"
#    }";
      content += "
#provider \"postgresql\" {
#  host            = local.pg_host
#  username        = local.pg_username
#  password        = local.pg_password
#}";
    }
    let mut have_mysql = false;
    if let Some(providers) = providers.clone() {
      if let Some(mysql) = providers.mysql {
        have_mysql = mysql;
      }
    }
    if have_mysql {
      requiered += "
    mysql = {
        source = \"petoju/mysql\"
        version = \"~> 3.0.43\"
    }";
      content += "
provider \"mysql\" {
  endpoint        = local.mysql_host
  username        = local.mysql_username
  password        = local.mysql_password
}";
    } else {
      requiered += "
#    mysql = {
#        source = \"petoju/mysql\"
#        version = \"~> 3.0.43\"
#    }";
      content += "
#provider \"mysql\" {
#  endpoint        = local.mysql_host
#  username        = local.mysql_username
#  password        = local.mysql_password
#}";
    }
    let mut have_http = false;
    if let Some(providers) = providers.clone() {
      if let Some(http) = providers.http {
        have_http = http;
      }
    }
    if have_http {
      requiered += "
      http = {
        source = \"hashicorp/http\"
        version = \"~> 3.3.0\"
    }";
      content += "
provider \"http\" {}";
    } else {
      requiered += "
#      http = {
#        source = \"hashicorp/http\"
#        version = \"~> 3.3.0\"
#    }";
      content += "
#provider \"http\" {}";
    }
    let mut have_gitea = false;
    if let Some(providers) = providers.clone() {
      if let Some(gitea) = providers.gitea {
        have_gitea = gitea;
      }
    }
    if have_gitea {
      requiered += "
      gitea = {
        source = \"Lerentis/gitea\"
        version = \"~> 0.16.0\"
      }";
      content += "
provider \"gitea\" {
  base_url = local.gitea_host
  username = local.gitea_username
  password = local.gitea_password
}";
    } else {
      requiered += "
#      gitea = {
#        source = \"Lerentis/gitea\"
#        version = \"~> 0.16.0\"
#      }";
      content += "
#provider \"gitea\" {
#  base_url = local.gitea_host
#  username = local.gitea_username
#  password = local.gitea_password
#}";
    }
    let mut have_restapi = false;
    if let Some(providers) = providers {
      if let Some(restapi) = providers.restapi {
        have_restapi = restapi;
      }
    }
    if have_restapi {
      requiered += "
      restapi = {
        source = \"Mastercard/restapi\"
        version = \"~> 1.18.0\"
      }";
    } else {
      requiered += "
#      restapi = {
#        source = \"Mastercard/restapi\"
#        version = \"~> 1.18.0\"
#      }";
    }
    file.push(dest_dir);
    file.push("providers.tf");
    requiered.push_str(content.as_str());
    gen_file(&file, &requiered, false)
}

pub fn save_to_tf(filename: &str, name: &str, str: &str) -> Result<()> {
  let content = match shell::get_output(&format!("echo 'jsondecode({:?})'|terraform console",str))  {Ok(d) => d, Err(e) => {bail!("{e}")}};
  gen_file(&filename.to_string().into(), &format!("variable \"{}\" {{
    default     = {}
  }}
  ",name, content), true)
}

pub fn gen_variables(dest_dir: &PathBuf, yaml: &Component,config:&serde_json::Map<String, serde_json::Value>, category: &str, component: &str, instance: &str) -> Result<()> {
  let mut file  = PathBuf::new();
  file.push(dest_dir);
  file.push("variables.tf");

  let mut content  = format!("
variable \"category\" {{
  default     = \"{}\"
}}
variable \"component\" {{
  default     = \"{}\"
}}
variable \"instance\" {{
  default     = \"{}\"
}}
variable \"install_owner\" {{
  default     = null
}}
", category, component, instance);
  for (name,value) in config {
      let str = serde_json::to_string(value).unwrap();
      let output = match shell::get_output(&format!("echo 'jsondecode({:?})'|terraform console",str))  {Ok(d) => d, Err(e) => {bail!("{e}")}};
      if yaml.tfaddtype.is_some() && *yaml.tfaddtype.as_ref().unwrap() {
          let typed = yaml.get_tf_type(name);
          tracing::debug!("{}({})={}", name, typed, output);
          content += format!("variable \"{}\" {{
  default     = {}
  type        = {}
}}
", name, output, typed).as_str();
      } else {
          content += format!("variable \"{}\" {{
  default     = {}
}}
", name, output).as_str();
      }
  }
  gen_file(&file, &content, false)
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct InstallOwner {
  pub namespace: String,
  pub name: String,
  pub uid: String,
}
impl InstallOwner {
  #[must_use] pub fn new(namespace: String, name: String,uid: String) -> InstallOwner {
    InstallOwner {
      namespace,
      name,
      uid
    }
  }
  pub fn to_string(&self) -> String {
    format!("[
  \"apiVersion\": \"vynil.solidite.fr/v1\",
  \"kind\": \"Install\",
  \"blockOwnerDeletion\": true,
  \"namespace\": \"{}\",
  \"name\": \"{}\",
  \"uid\": \"{}\",
]", self.namespace, self.name, self.uid)
  }
}

pub fn gen_tfvars(dest_dir: &PathBuf, config:&serde_json::Map<String, serde_json::Value>, owner: Option<InstallOwner>) -> Result<()> {
    let mut file  = PathBuf::new();
    file.push(dest_dir);
    file.push("env.tfvars");

    let mut content: String = String::new();
    for (name,value) in config {
      let str = serde_json::to_string(value).unwrap();
      let output = match shell::get_output(&format!("echo 'jsondecode({:?})'|terraform console",str))  {Ok(d) => d, Err(e) => {bail!("{e}")}};
      tracing::debug!("{}={}", name, output);
      content += format!("{} = {}
", name, output).as_str();
    }
    if let Some(ownref) = owner {
      let str = serde_json::to_string(&ownref).unwrap();
      let output = match shell::get_output(&format!("echo 'jsondecode({:?})'|terraform console",str))  {Ok(d) => d, Err(e) => {bail!("{e}")}};

      content += format!("install_owner = [{}]
", output).as_str();
    }
    gen_file(&file, &content, true)
}

pub fn have_datas(dest_dir: &PathBuf) -> bool {
  let mut file  = PathBuf::new();
  file.push(dest_dir);
  file.push("datas.tf");
  Path::new(&file).exists()
}

pub fn gen_datas(dest_dir: &PathBuf) -> Result<()> {
    let mut file  = PathBuf::new();
    file.push(dest_dir);
    file.push("datas.tf");
    gen_file(&file, &"
locals {
#   authentik_url = \"http://authentik.${var.domain}-auth.svc\"
#   authentik_token = data.kubernetes_secret_v1.authentik.data[\"AUTHENTIK_BOOTSTRAP_TOKEN\"]
#   gitea_host = \"http://gitea-http.${var.domain}-ci.svc:3000/\"
#   gitea_username = data.kubernetes_secret_v1.gitea.data[\"username\"]
#   gitea_password = data.kubernetes_secret_v1.gitea.data[\"password\"]
  common-labels = {
    \"vynil.solidite.fr/owner-name\" = var.instance
    \"vynil.solidite.fr/owner-namespace\" = var.namespace
    \"vynil.solidite.fr/owner-category\" = var.category
    \"vynil.solidite.fr/owner-component\" = var.component
    \"app.kubernetes.io/managed-by\" = \"vynil\"
    \"app.kubernetes.io/name\" = var.component
    \"app.kubernetes.io/instance\" = var.instance
  }
#   pvc_spec = merge({
#     \"accessModes\" = [var.storage.volume.accessMode]
#     \"volumeMode\" = var.storage.volume.type
#     \"resources\" = {
#       \"requests\" = {
#         \"storage\" = \"${var.storage.volume.size}\"
#       }
#     }
#   }, var.storage.volume.class != \"\" ?{
#     \"storageClassName\" = var.storage.volume.class
#   }:{})
}

# data \"kubernetes_secret_v1\" \"postgresql_password\" {
#   depends_on = [kubernetes_manifest.prj_postgresql]
#   metadata {
#     name = \"${var.instance}-${var.component}-pg-app\"
#     namespace = var.namespace
#   }
# }

# data \"kubernetes_secret_v1\" \"prj_mysql_secret\" {
#  depends_on = [kubectl_manifest.prj_mysql_secret]
#  metadata {
#    name      = \"${local.app_slug}-mysql\"
#    namespace = var.namespace
#    labels = local.mysql_labels
#  }
# }

# data \"kubernetes_secret_v1\" \"authentik\" {
#   metadata {
#     name = \"authentik\"
#     namespace = \"${var.domain}-auth\"
#   }
# }

# data \"kubernetes_ingress_v1\" \"authentik\" {
#   metadata {
#     name = \"authentik\"
#     namespace = \"${var.domain}-auth\"
#   }
# }

data \"kustomization_overlay\" \"data\" {
  common_labels = local.common-labels
  namespace = var.namespace
  resources = [for file in fileset(path.module, \"*.yaml\"): file if file != \"index.yaml\"]
}
".to_string(), false)
}

pub fn gen_ressources(dest_dir: &PathBuf) -> Result<()> {
    let mut file  = PathBuf::new();
    file.push(dest_dir);
    file.push("ressources.tf");
    gen_file(&file, &"
# first loop through resources in ids_prio[0]
resource \"kustomization_resource\" \"pre\" {
  for_each = data.kustomization_overlay.data.ids_prio[0]

  manifest = (
    contains([\"_/Secret\"], regex(\"(?P<group_kind>.*/.*)/.*/.*\", each.value)[\"group_kind\"])
    ? sensitive(data.kustomization_overlay.data.manifests[each.value])
    : data.kustomization_overlay.data.manifests[each.value]
  )
}

# then loop through resources in ids_prio[1]
# and set an explicit depends_on on kustomization_resource.pre
# wait 2 minutes for any deployment or daemonset to become ready
resource \"kustomization_resource\" \"main\" {
  for_each = data.kustomization_overlay.data.ids_prio[1]

  manifest = (
    contains([\"_/Secret\"], regex(\"(?P<group_kind>.*/.*)/.*/.*\", each.value)[\"group_kind\"])
    ? sensitive(data.kustomization_overlay.data.manifests[each.value])
    : data.kustomization_overlay.data.manifests[each.value]
  )
  wait = true
  timeouts {
    create = \"5m\"
    update = \"5m\"
  }

  depends_on = [kustomization_resource.pre]
}

# finally, loop through resources in ids_prio[2]
# and set an explicit depends_on on kustomization_resource.main
resource \"kustomization_resource\" \"post\" {
  for_each = data.kustomization_overlay.data.ids_prio[2]

  manifest = (
    contains([\"_/Secret\"], regex(\"(?P<group_kind>.*/.*)/.*/.*\", each.value)[\"group_kind\"])
    ? sensitive(data.kustomization_overlay.data.manifests[each.value])
    : data.kustomization_overlay.data.manifests[each.value]
  )

  depends_on = [kustomization_resource.main]
}
".to_string(), false)
}
