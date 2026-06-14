# Reference

## Operator Environment Variables

| Variable | Default | Role |
|---|---|---|
| `CONTROLLER_BASE_DIR` | `./operator` | Directory for Handlebars templates. |
| `VYNIL_NAMESPACE` | `vynil-system` | Vynil system namespace. |
| `AGENT_IMAGE` | compiled default image | Agent image used for Jobs. |
| `AGENT_ACCOUNT` | `vynil-agent` | ServiceAccount for agent Jobs. |
| `AGENT_LOG_LEVEL` | `info` | Log level for agent Jobs. |
| `TENANT_LABEL` | `vynil.solidite.fr/tenant` | Label key identifying a tenant. |
| `SCAN_PACKAGE` | (absent) | Partial filter for `box scan` / `box file-scan`. |

> `AGENT_IMAGE` must match the operator version. Check the actual deployed value rather
> than a hardcoded value in documentation.

## Agent Environment Variables

See the equivalent flags in the [CLI Reference](../cli.md): `NAMESPACE`, `INSTANCE`,
`VYNIL_NAMESPACE`, `PACKAGE_DIRECTORY`, `SCRIPT_DIRECTORY`, `TEMPLATE_DIRECTORY`,
`CONFIG_DIR`, `CONTROLLER_VALUES`, `AGENT_IMAGE`, `TAG`, `LOG_LEVEL`, `SIGNING_KEY`,
`JUNIT_OUTPUT_FILENAME`, `TEMPLATE_OUTPUT_FILENAME`, `TESTSETS_DIRECTORY`, `TEST_NAME`.

## Prometheus Metrics

Exposed at `GET /metrics` (port 9000, OpenMetrics format). Four registries (JukeBox,
System, Service, Tenant) expose per type:

- reconciliation duration (histogram);
- success/failure counters;
- in-progress reconciliation gauge;
- last event timestamp.

## Operator Handlebars Templates

Directory [`operator/templates/`](../../../operator/templates/):

| Template | Usage |
|---|---|
| `package.yaml.hbs` | Instance install/delete Job. |
| `cronscan.yaml.hbs` | JukeBox scan CronJob. |
| `scan.yaml.hbs` | JukeBox manual scan Job. |

Variables always available in context: `tag`, `image`, `registry`, `namespace`, `name`,
`job_name`, `package_type`, `package_action`, `digest`, `ctrl_values`, `rec_crds`,
`rec_system_services`, `rec_tenant_services`.

## Package Handlebars Helpers

| Helper | Signature | Description |
|---|---|---|
| `image_from_ctx` | `(ctx "key")` | `registry/repository:tag` from `package.yaml[images][key]`. |
| `resources_from_ctx` | `(ctx "key")` | `{requests, limits}` from `package.yaml[resources][key]`. |
| `selector_from_ctx` | `(ctx comp="key")` | Selector labels for the component. |
| `labels_from_ctx` | `(ctx)` | Full pod template labels. |
| `json_to_str` | `(value)` | Serializes an object to inline JSON. |
| `ctx_have_crd` | `(ctx "group/version/kind")` | True if the CRD is installed. |

See [Package generation](../gen-package.md) for full usage.

## YAML Strategy

| Usage | Library | Key order |
|---|---|---|
| Rust code (serde) | `serde_yaml` | alphabetical |
| `yaml_*_ordered` (Rhai) | `rust-yaml` | preserved |

`rust-yaml` is used for `package.yaml` (order preservation, block scalars intact);
`serde_yaml` everywhere else. The `YamlError(String)` type wraps both.

## Repository Structure

```text
vynil/
├── common/      shared library (CRDs, Rhai/Handlebars engines, handlers)
├── operator/    Kubernetes controller (+ templates)
├── agent/       CLI executed in Jobs (+ Rhai scripts)
├── box/         source packages (vynil, test)
├── deploy/      kustomize: CRDs + bootstrap
└── docs/        this documentation
```

See [Architecture](../architecture.md) for crate details.
