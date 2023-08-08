

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

pub fn gen_ingress(dest_dir: &PathBuf) -> Result<()> {
    if ! Path::new(dest_dir).is_dir() {
        bail!("{:?} is not a directory", dest_dir);
    }
    let mut file  = PathBuf::new();
    file.push(dest_dir);
    file.push("ingress.tf");
    gen_file(&file, &"
locals {
    dns-names = [\"${var.sub-domain}.${var.domain-name}\"]
    middlewares = [\"${var.instance}-https\"]
    service = {
      \"name\"  = \"${var.instance}\"
      \"port\" = {
        \"number\" = 80
      }
    }
    rules = [ for v in local.dns-names : {
      \"host\" = \"${v}\"
      \"http\" = {
        \"paths\" = [{
          \"backend\"  = {
            \"service\" = local.service
          }
          \"path\"     = \"/\"
          \"pathType\" = \"Prefix\"
        }]
      }
    }]
}

resource \"kubectl_manifest\" \"prj_certificate\" {
  yaml_body  = <<-EOF
    apiVersion: \"cert-manager.io/v1\"
    kind: \"Certificate\"
    metadata:
      name: \"${var.instance}\"
      namespace: \"${var.namespace}\"
      labels: ${jsonencode(local.common-labels)}
    spec:
        secretName: \"${var.instance}-cert\"
        dnsNames: ${jsonencode(local.dns-names)}
        issuerRef:
          name: \"${var.issuer}\"
          kind: \"ClusterIssuer\"
          group: \"cert-manager.io\"
  EOF
}

resource \"kubectl_manifest\" \"prj_https_redirect\" {
  yaml_body  = <<-EOF
    apiVersion: \"traefik.containo.us/v1alpha1\"
    kind: \"Middleware\"
    metadata:
      name: \"${var.instance}-https\"
      namespace: \"${var.namespace}\"
      labels: ${jsonencode(local.common-labels)}
    spec:
      redirectScheme:
        scheme: \"https\"
        permanent: true
  EOF
}

resource \"kubectl_manifest\" \"prj_ingress\" {
  force_conflicts = true
  yaml_body  = <<-EOF
    apiVersion: \"networking.k8s.io/v1\"
    kind: \"Ingress\"
    metadata:
      name: \"${var.instance}\"
      namespace: \"${var.namespace}\"
      labels: ${jsonencode(local.common-labels)}
      annotations:
        \"traefik.ingress.kubernetes.io/router.middlewares\": \"${join(\",\", [for m in local.middlewares : format(\"%s-%s@kubernetescrd\", var.namespace, m)])}\"
    spec:
      ingressClassName: \"${var.ingress-class}\"
      rules: ${jsonencode(local.rules)}
      tls:
      - hosts: ${jsonencode(local.dns-names)}
        secretName: \"${var.instance}-cert\"
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
