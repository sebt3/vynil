# Package Format

A package exists in two forms: a **source directory** (during development) and
an **OCI image** (once packaged). This page describes both.

## Source directory

```text
my-package/
├── package.yaml          # manifest (metadata, images, resources, options, requirements)
├── befores/              # phase 1 — .yaml.hbs / .yaml templates
├── vitals/               # phase 2 — persistent data (PVC)
├── tofu/                 # phase 3 — OpenTofu/Terraform files (*.tf)
├── others/               # phase 4 — Service, ConfigMap, Ingress, Role…
├── scalables/            # phase 5 — Deployment, StatefulSet…
├── posts/                # phase 6 — final actions
├── crds/                 # CRDs (system/service only)
├── handlebars/           # reusable Handlebars partials
└── scripts/              # lifecycle Rhai hooks (*.rhai)
```

Phase directories are **optional**: the agent only processes those that are present (see
[Lifecycle](lifecycle.md)). `*.yaml.hbs` files are rendered with the instance context;
`*.yaml` files are applied as-is.

> Naming note: the generator (`gen_package.rhai`) writes to directories prefixed with
> `get_` (`get_vitals/`, `get_scalables/`, `get_others/`, `get_systems/`, `get_crds/`)
> which correspond to the phases above on the agent side.

## `package.yaml`

```yaml
---
apiVersion: vinyl.solidite.fr/v1beta1
kind: Package
metadata:
  name: traefik             # identifier (appslug in templates)
  category: networking      # free-form category
  type: system              # system | service | tenant
  app_version: "3.7.1"      # application version
  description: Traefik ingress controller.
  features:
    - upgrade
    - auto_config
  # backup_affinity: controller   # component serving as required pod affinity for backup jobs
images:
  traefik:                  # arbitrary key, referenced by {{image_from_ctx this "traefik"}}
    registry: ghcr.io
    repository: traefik/traefik
    tag: v3.7.1             # updated by `agent package update`
resources:                  # requests/limits per container
  traefik:
    requests: { cpu: 100m, memory: 128Mi }
    limits:   { cpu: 1000m, memory: 256Mi }
requirements: []            # dependencies and prerequisites
recommandations: []         # optional dependencies that trigger an update on change (e.g. monitoring)
options:                    # schema for configurable parameters
  replicas:
    type: integer
    default: 1
    description: Number of replicas
```

### Metadata (`metadata`)

| Field | Required | Description |
|---|---|---|
| `name` | yes | Package identifier (becomes `instance.appslug`). |
| `category` | yes | Grouping category. |
| `type` | yes | `system`, `service`, or `tenant`. |
| `app_version` | recommended | Version of the bundled application. |
| `description` | yes | Human-readable description. |
| `features` | no | `upgrade`, `backup`, `monitoring`, `high_availability`, `auto_config`, `auto_scaling`, `deprecated`. |
| `backup_affinity` | no | Component used as required pod affinity for backup jobs. |

### Requirements (`requirements`)

List of constraints checked by the operator before installing
(`VynilPackageRequirement`):

| Variant | Checks |
|---|---|
| `MinimumPreviousVersion` | minimum already-installed version required to allow the upgrade |
| `VynilVersion` | minimum Vynil framework version |
| `ClusterVersion` | minimum Kubernetes version |
| `CustomResourceDefinition` | presence of a given CRD |
| `SystemService` / `TenantService` | presence of a service provided by another package |
| `Cpu` / `Memory` / `Disk` | available resources |
| `StorageCapability` | storage capability (`RWX` / `ROX`) |
| `Prefly` | custom Rhai verification script |

### Options (`options`)

OpenAPI-style schema for accepted parameters in `spec.options` of instances. Options
are validated (`validate_options`) and then exposed to the rendering context. Any value
provided in `spec.options` is user input: the rendering must take this into account.

### Recommendations & `value_script`

- `recommandations`: optional lists (CRDs, system/tenant services) whose presence
  activates additional features without being blocking.
- `value_script`: Rhai script evaluated by the operator to produce control values
  (`ctrl_values`) injected into the Handlebars context.

## OCI image (packaged package)

`agent package build` (or `package unpack` for the inverse) turns the directory into
an OCI image. Metadata is carried by **OCI annotations**:

| Annotation | Content (JSON, unless stated otherwise) |
|---|---|
| `fr.solidite.vynil.metadata` | name, category, type, app_version, features |
| `fr.solidite.vynil.requirements` | list of prerequisites |
| `fr.solidite.vynil.options` | options schema |
| `fr.solidite.vynil.recommandations` | recommendations |
| `fr.solidite.vynil.value_script` | Rhai script (string) |

The image content (the layer) includes the phase directories, Rhai scripts, and
Handlebars templates. The agent mounts this content (`unpack`) before executing the
lifecycle.

See [Package generation](../gen-package.md) to produce these directories from a Helm
chart or raw manifests, and [Build & signing](../build-signing.md) for signed publishing.
