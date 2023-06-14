

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
options:
  sub-domain:
    default:
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
        registry:
        repository:
        tag:
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
    middlewares = [{\"name\" = \"${var.instance}-https\"}]
    services = [{
      \"kind\" = \"Service\"
      \"name\" = \"${var.instance}\"
      \"namespace\" = var.namespace
      \"port\" = 80
    }]
    routes = [ for v in local.dns-names : {
      \"kind\" = \"Rule\"
      \"match\" = \"Host(`${v}`)\"
      \"middlewares\" = local.middlewares
      \"services\" = local.services
    }]
}

resource \"kubernetes_manifest\" \"prj_certificate\" {
  manifest = {
    apiVersion = \"cert-manager.io/v1\"
    kind       = \"Certificate\"
    metadata   = {
      name      = \"${var.instance}\"
      namespace = var.namespace
      labels    = local.common-labels
    }
    spec = {
        secretName = \"${var.instance}-cert\"
        dnsNames   = local.dns-names
        issuerRef  = {
          name  = var.issuer
          kind  = \"ClusterIssuer\"
          group = \"cert-manager.io\"
        }
    }
  }
}

resource \"kubernetes_manifest\" \"prj_https_redirect\" {
  manifest = {
    apiVersion = \"traefik.containo.us/v1alpha1\"
    kind       = \"Middleware\"
    metadata   = {
      name      = \"${var.instance}-https\"
      namespace = var.namespace
      labels    = local.common-labels
    }
    spec = {
      redirectScheme = {
        scheme = \"https\"
        permanent = true
      }
    }
  }
}

resource \"kubernetes_manifest\" \"prj_ingress\" {
  field_manager {
    force_conflicts = true
  }
  manifest = {
    apiVersion = \"traefik.containo.us/v1alpha1\"
    kind       = \"IngressRoute\"
    metadata = {
      name      = \"${var.instance}\"
      namespace = var.namespace
      labels    = local.common-labels
      annotations = {
        \"kubernetes.io/ingress.class\" = var.ingress-class
      }
    }
    spec = {
      entryPoints = [\"web\",\"websecure\"]
      routes = local.routes
      tls = {
        secretName = \"${var.instance}-cert\"
      }
    }
  }
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
resource \"kubernetes_manifest\" \"prj_postgresql\" {
  manifest = {
    apiVersion = \"acid.zalan.do/v1\"
    kind       = \"postgresql\"
    metadata = {
      name      = \"${var.instance}-${var.component}\"
      namespace = var.namespace
      labels    = local.common-labels
    }
    spec = {
      databases = {
        \"${var.component}\" = \"${var.component}\"
      }
      numberOfInstances = var.postgres.replicas
      podAnnotations = {
        \"k8up.io/backupcommand\" = \"pg_dump -U postgres -d ${var.component} --clean\"
        \"k8up.io/file-extension\" = \".sql\"
      }
      postgresql = {
        version = var.postgres.version
      }
      teamId = var.instance
      users = {
        \"${var.component}\" = [
          \"superuser\",
          \"createdb\"
        ]
      }
      volume = {
        size = var.postgres.storage
      }
    }
  }
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
resource \"kubernetes_manifest\" \"prj_secret\" {
  manifest = {
    apiVersion = \"secretgenerator.mittwald.de/v1alpha1\"
    kind       = \"StringSecret\"
    metadata = {
      name      = var.component
      namespace = var.namespace
      labels    = local.common-labels
    }
    spec = {
      forceRegenerate = false,
      data = {
        username = var.admin.name
      }
      fields = [
        {
          fieldName = \"password\"
          length    = \"32\"
        }
      ]
    }
  }
}
".to_string(), false)
}
