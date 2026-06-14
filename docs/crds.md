# CRD Reference

All resources are in the group **`vynil.solidite.fr/v1`**. The authoritative definitions
are in [`common/src/`](../../common/src/) (Rust types deriving `CustomResource`) and are
generated into [`deploy/crd/crd.yaml`](../../deploy/crd/crd.yaml) via `agent crdgen`.

## JukeBox (cluster-scoped)

Package source. Shortcut: `jb`.

```yaml
apiVersion: vynil.solidite.fr/v1
kind: JukeBox
metadata:
  name: home-alpha
spec:
  source:            # exactly one variant (see JukeBox Sources)
    list: ["registry.example.com/org/vynil"]
  maturity: stable   # stable | beta | alpha
  schedule: "0 3 * * *"
  pull_secret: my-pull-secret   # optional: dockerconfigjson Secret
status:
  packages: []       # cache of scanned packages (waypoints)
```

| Field | Type | Description |
|---|---|---|
| `spec.source` | object | Source variant: `list`, `harbor`, `gitlab`, `script`, `http`, `s3`. |
| `spec.maturity` | enum | Maturity level used during scan. |
| `spec.schedule` | cron | Rescan schedule (CronJob). |
| `spec.pull_secret` | string | `dockerconfigjson` Secret for private registry. |
| `status.packages` | list | Computed catalogue (one waypoint per upgrade epoch). |

## SystemInstance (namespaced)

System package installation (cluster component). No backup, no `initFrom`.

```yaml
apiVersion: vynil.solidite.fr/v1
kind: SystemInstance
metadata:
  name: traefik
  namespace: vynil-system
spec:
  jukebox: vynil
  category: networking
  package: traefik
  options: {}
status:
  tag: "3.7.1"
  digest: "<options fingerprint>"
  conditions: []
```

## ServiceInstance (namespaced)

Installation of a **service** package (shared application, own CRDs, backup).
Same structure as `TenantInstance` below (with `initFrom`).

## TenantInstance (namespaced)

Installation of a **tenant** package. Shortcut: `vti`.

```yaml
apiVersion: vynil.solidite.fr/v1
kind: TenantInstance
metadata:
  name: gretel
  namespace: epikaf-nan-ia
spec:
  jukebox: home-alpha
  category: think
  package: ollama
  initFrom:                 # optional: restore from a backup
    secretName: backup-settings
    subPath: epikaf-nan-ia/ollama
    snapshot: "abc123"
    version: "0.1.8"        # package version to use for restore
  options:
    use_rocm: true
status:
  tag: "0.1.8-beta.50"
  digest: "<options fingerprint>"
  conditions: []
  vitals:    []   # created PVCs
  scalables: []   # created Deployment/StatefulSet
  others:    []   # created Service/ConfigMap/Ingress/…
  befores:   []
  posts:     []
  services:  []   # published services (capability registry)
  tfstate:   "…"  # OpenTofu state (gzip+base64), if applicable
  rhaistate: "…"  # custom Rhai state (gzip+base64), if applicable
```

### Common spec (service/tenant)

| Field | Type | Description |
|---|---|---|
| `spec.jukebox` | string | Name of the source JukeBox. |
| `spec.category` | string | Package category. |
| `spec.package` | string | Package name. |
| `spec.options` | map | Parameters validated against the package `options` schema. |
| `spec.initFrom.secretName` | string | S3/Restic Secret (default `backup-settings`). |
| `spec.initFrom.subPath` | string | Prefix in the bucket (default `<ns>/<app-slug>`). |
| `spec.initFrom.snapshot` | string | Restic snapshot identifier to restore. |
| `spec.initFrom.version` | string | Exact package version for the restore. |

### Status conditions

The `status.conditions` reflects progress. Possible types (tenant): `Ready`,
`Installed`, `Backuped`, `Restored`, `AgentStarted`, `TofuInstalled`, `BeforeApplied`,
`VitalApplied`, `ScalableApplied`, `InitFrom`, `ScheduleBackup`, `OtherApplied`,
`RhaiApplied`, `PostApplied`. Each condition carries a `status` (`True`/`False`), a
`message`, a `generation`, and a `lastTransitionTime`.

Example of an observable error message: an `AgentStarted=False` condition with
`message: "Package think/ollama is missing"` indicates that the operator did not find the
matching package in the JukeBox cache.

## Control annotations

### On instances

| Annotation | Value | Effect |
|---|---|---|
| `vynil.solidite.fr/suspend` | `"true"` | Suspends reconciliation (requeue 15 min, no action) until removed. |
| `vynil.solidite.fr/force-reinstall` | present | Deletes the existing Job and forces a reinstallation; the annotation is removed automatically. |

### On JukeBox resources

| Annotation | Value | Behavior |
|---|---|---|
| `vynil.solidite.fr/force-scan` | `"true"` (or present) | Full scan. |
| `vynil.solidite.fr/force-scan` | `"<category>"` | Partial scan of a category. |
| `vynil.solidite.fr/force-scan` | `"<category>/<name>"` | Partial scan of a single package. |
| `vynil.solidite.fr/last-scan-time` | (managed by the operator) | Completion timestamp of the last processed scan. |

## Finalizers

Each resource places a finalizer (`<kind>.vynil.solidite.fr`) to guarantee cleanup of
child objects and Jobs before deletion. See
[Reconciliation](reconciliation.md) and [Troubleshooting](operations/troubleshooting.md).
