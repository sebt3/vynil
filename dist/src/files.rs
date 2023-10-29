

use std::path::{PathBuf, Path};
use anyhow::{Result, bail};
use package::terraform::gen_file;

pub fn gen_index_yaml(dest_dir: &PathBuf) -> Result<()> {
    if ! Path::new(dest_dir).is_dir() {
        bail!("{:?} is not a directory", dest_dir);
    }
    let mut file  = PathBuf::new();
    file.push(dest_dir);
    file.push("index.yaml");
    gen_file(&file, &"
apiVersion: vinyl.solidite.fr/v1beta1
kind: Component
category:
metadata:
  name:
  description:
providers:
  authentik: true
  kubernetes: true
  kubectl: true
options:
  sub-domain:
    default: to-be-set
  domain-name:
    default: your_company.com
  domain:
    default: your-company
  issuer:
    default: letsencrypt-prod
  ingress-class:
    default: traefik
  app-group:
    default: infra
  storage:
    default:
      size: 1Gi
      accessMode: ReadWriteOnce
      type: Filesystem
    properties:
      type:
        enum:
          - Filesystem
          - Block
      accessMode:
        enum:
          - ReadWriteOnce
          - ReadOnlyMany
          - ReadWriteMany
  images:
    default:
      operator:
        registry: docker.io
        repository: to-be/defined
        tag: v1.0.0
        pullPolicy: IfNotPresent
    properties:
      operator:
        properties:
          pullPolicy:
            enum:
            - Always
            - Never
            - IfNotPresent
".to_string(), false)
}

pub fn gen_index_rhai(dest_dir: &PathBuf) -> Result<()> {
    if ! Path::new(dest_dir).is_dir() {
        bail!("{:?} is not a directory", dest_dir);
    }
    let mut file  = PathBuf::new();
    file.push(dest_dir);
    file.push("index.rhai");
    gen_file(&file, &"
const VERSION=config.release;
const SRC=src;
const DEST=dest;
fn pre_pack() {
    shell(\"helm repo add goauthentik https://charts.goauthentik.io/\");
    shell(`helm template authentik goauthentik/authentik --namespace=vynil-auth --values values.yml >${global::SRC}/chart.yaml`);
    shell(`kubectl kustomize https://github.com/rabbitmq/cluster-operator//config/manager/?ref=${global::VERSION} >${global::SRC}/manager.yaml`);
}
fn post_pack() {
    shell(`rm -f ${global::DEST}/v1_Secret_authentik.yaml`);
}
fn pre_install() {
    shell(`kubectl apply -k https://github.com/rabbitmq/cluster-operator//config/crd/?ref=v${global::VERSION}`);
}
".to_string(), false)
}

pub fn gen_presentation(dest_dir: &PathBuf) -> Result<()> {
    if ! Path::new(dest_dir).is_dir() {
        bail!("{:?} is not a directory", dest_dir);
    }
    let mut file  = PathBuf::new();
    file.push(dest_dir);
    file.push("presentation.tf");
    gen_file(&file, &"
locals {
  dns-name = \"${var.sub-domain}.${var.domain-name}\"
  dns-names = [local.dns-name]
  app-name = var.component == var.instance ? var.instance : format(\"%s-%s\", var.component, var.instance)
  icon              = \"pics/logo.svg\"
  request_headers = {
    \"Content-Type\"  = \"application/json\"
    Authorization   = \"Bearer ${data.kubernetes_secret_v1.authentik.data[\"AUTHENTIK_BOOTSTRAP_TOKEN\"]}\"
  }
  service           = {
    \"name\"  = \"${var.component}-${var.instance}\"
    \"port\" = {
      \"number\" = 80
    }
  }
}

module \"service\" {
  source = \"/dist/modules/service\"
  component         = var.component
  instance          = var.instance
  namespace         = var.namespace
  labels            = local.common-labels
  target            = \"http\"
  port              = local.service.port.number
  providers = {
    kubectl = kubectl
  }
}

module \"ingress\" {
  source = \"/dist/modules/ingress\"
  component         = \"\"
  instance          = var.instance
  namespace         = var.namespace
  issuer            = var.issuer
  ingress-class     = var.ingress-class
  labels            = local.common-labels
  dns-names         = local.dns-names
  middlewares       = [\"forward-${local.app-name}\"]
  service           = local.service
  providers = {
    kubectl = kubectl
  }
}

module \"application\" {
  source = \"/dist/modules/application\"
  component         = var.component
  instance          = var.instance
  app-group         = var.app-group
  dns-name          = local.dns-name
  icon              = local.icon
  protocol_provider = module.forward.provider-id
  providers = {
    authentik = authentik
  }
}

provider \"restapi\" {
  uri = \"http://authentik.${var.domain}-auth.svc/api/v3/\"
  headers = local.request_headers
  create_method = \"PATCH\"
  update_method = \"PATCH\"
  destroy_method = \"PATCH\"
  write_returns_object = true
  id_attribute = \"name\"
}

module \"forward\" {
  source = \"/dist/modules/forward\"
  component         = var.component
  instance          = var.instance
  domain            = var.domain
  namespace         = var.namespace
  ingress-class     = var.ingress-class
  labels            = local.common-labels
  dns-names         = local.dns-names
  service           = local.service
  icon              = local.icon
  request_headers   = local.request_headers
  providers = {
    restapi = restapi
    http = http
    kubectl = kubectl
    authentik = authentik
  }
}
  EOF
}
".to_string(), false)
}

pub fn gen_postgresql(dest_dir: &PathBuf) -> Result<()> {
    if ! Path::new(dest_dir).is_dir() {
        bail!("{:?} is not a directory", dest_dir);
    }
    let mut file  = PathBuf::new();
    file.push(dest_dir);
    file.push("postgresql.tf");
    gen_file(&file, &"
locals {
  pg-labels = merge(local.common-labels, {
    \"app.kubernetes.io/component\" = \"pg\"
  })
  pool-labels = merge(local.common-labels, {
    \"app.kubernetes.io/component\" = \"pg-pool\"
  })
}
resource \"kubectl_manifest\" \"prj_pg\" {
  yaml_body  = <<-EOF
    apiVersion: postgresql.cnpg.io/v1
    kind: Cluster
    metadata:
      name: \"${var.instance}-${var.component}-pg\"
      namespace: \"${var.namespace}\"
      labels: ${jsonencode(local.pg-labels)}
    spec:
      instances: ${var.postgres.replicas}
      storage:
        size: \"${var.postgres.storage}\"
  EOF
}
resource \"kubectl_manifest\" \"prj_pg_pool\" {
  depends_on = [kubectl_manifest.prj_pg]
  yaml_body  = <<-EOF
    apiVersion: postgresql.cnpg.io/v1
    kind: Pooler
    metadata:
      name: \"${var.instance}-${var.component}-pool\"
      namespace: \"${var.namespace}\"
      labels: ${jsonencode(local.pool-labels)}
    spec:
      cluster:
        name: \"${var.instance}-${var.component}-pg\"
      instances: ${var.postgres.replicas}
      type: rw
      pgbouncer:
        poolMode: session
        parameters:
          max_client_conn: \"1000\"
          default_pool_size: \"10\"
  EOF
}
".to_string(), false)
}

pub fn gen_secret(dest_dir: &PathBuf) -> Result<()> {
    if ! Path::new(dest_dir).is_dir() {
        bail!("{:?} is not a directory", dest_dir);
    }
    let mut file  = PathBuf::new();
    file.push(dest_dir);
    file.push("secret.tf");
    gen_file(&file, &"
resource \"kubectl_manifest\" \"prj_secret\" {
  ignore_fields = [\"metadata.annotations\"]
  yaml_body  = <<-EOF
    apiVersion: \"secretgenerator.mittwald.de/v1alpha1\"
    kind: \"StringSecret\"
    metadata:
      name: \"${var.component}\"
      namespace: \"${var.namespace}\"
      labels: ${jsonencode(local.common-labels)}
    spec:
      forceRegenerate: false
      data:
        username: \"${var.component}\"
      fields:
      - fieldName: \"password\"
        length: \"32\"
      - fieldName: \"jwt-secret\"
        length: \"128\"
  EOF
}
".to_string(), false)
}
