

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
dependencies:
# - category: core
#   component: secret-generator
# - category: share
#   component: authentik
# - category: dbo
#   component: pg
# - category: dbo
#   component: redis
providers:
  kubernetes: true
  kubectl: true
  authentik: true
  http: true
  restapi: true
options:
  timezone:
    default: Europe/Paris
  language:
    default: fr_FR
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
  replicas:
    default: 1
  hpa:
    default:
      min-replicas: 1
      max-replicas: 5
      avg-cpu: 50
  backups:
    default:
      enable: false
      use-barman: false
      endpoint: \"\"
      secret-name: backup-settings
      key-id-key: s3-id
      secret-key: s3-secret
      restic-key: bck-password
      schedule:
        db: \"10 3 * * *\"
        backup: \"10 3 * * *\"
        check:  \"10 5 * * 1\"
        prune:  \"10 1 * * 0\"
      retention:
        db: \"30d\"
        keepDaily: 14
        keepMonthly: 12
        keepWeekly: 6
        keepYearly: 12
  postgres:
    default:
      replicas: 1
  redis:
    default:
      exporter:
        enabled: true
  storage:
    description: Configure this app storage
    default:
      volume:
        size: 1Gi
        accessMode: ReadWriteOnce
        type: Filesystem
      redis:
        size: '2Gi'
      postgres:
        size: '10Gi'
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
      postgresql:
        registry: ghcr.io
        repository: cloudnative-pg/postgresql
        tag: 15.3
      redis:
        registry: quay.io
        repository: opstree/redis
        tag: v7.0.12
        pullPolicy: IfNotPresent
      redis_exporter:
        registry: quay.io
        repository: opstree/redis-exporter
        tag: v1.44.0
        pullPolicy: IfNotPresent
      app:
        registry: docker.io
        repository: to-be/defined
        tag: v1.0.0
        pullPolicy: IfNotPresent
    properties:
      app:
        properties:
          pullPolicy:
            enum:
            - Always
            - Never
            - IfNotPresent
      redis:
        properties:
          pullPolicy:
            enum:
            - Always
            - Never
            - IfNotPresent
      redis_exporter:
        properties:
          pullPolicy:
            enum:
            - Always
            - Never
            - IfNotPresent
".to_string(), false)
}

