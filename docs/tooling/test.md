# Package Tests

Vynil distinguishes two testing levels: **package tests** (`agent package test`,
written by the package author) and **internal regression tests** (agent Rust test suite,
ensuring Rhai engine stability).

## Package tests — `agent package test`

Runs scenarios defined in `<package-dir>/tests/` with a real Rhai engine and
**K8s/HTTP/OCI mocks**: no cluster required. Template rendering and lifecycle hooks are
executed, and assertions validate the result.

```bash
# all tests
agent package test -p ./my-package --all

# a specific test, with rendered output dump
agent package test -p ./my-package --test-name install-default \
  --template-output-filename rendered.yaml

# JUnit report for CI
agent package test -p ./my-package --all --format json \
  --junit-output-filename results.xml
```

| Option | Role |
|---|---|
| `--test-name <name>` | Runs a single test. |
| `--all` | Runs all tests. |
| `--testsets-dir <dir>` | Additional test set directory. |
| `--format text\|json` | Output format. |
| `--junit-output-filename <file>` | JUnit XML report. |
| `--template-output-filename <file>` | Rendered output dump (single test only). |

The `tests/` directory must exist, otherwise the agent returns `MissingTestDirectory`.

### What a test can do

- provide instance **options** and a **context** (cluster/tenant);
- declare **mocks**: K8s objects present, HTTP responses, OCI manifests;
- execute a flow (`context → install`, `context → delete`, …);
- **assert** on created objects, status, or template rendering.

### Variables available in assertion selectors

Selectors (`selector.name`, `selector.namespace`) and expected values (`value`) are
Handlebars templates resolved against the **real context** produced by `ctx::run()` — the
same context used by the package's Rhai scripts and templates. All context keys are
available, notably:

| Variable | Description |
|---|---|
| `{{values.xxx}}` | Instance options merged with `package.yaml` defaults |
| `{{defaults.xxx}}` | Raw default values from `package.yaml` |
| `{{cluster.ha}}` | `true` if the cluster has more than one node |
| `{{cluster.storage_classes}}` | List of mocked StorageClasses |
| `{{tenant.name}}` | Tenant name (tenant-type packages only) |
| `{{tenant.namespaces}}` | Tenant namespaces |
| `{{instance.appslug}}` | Instance slug (`{instance}-{package}`, truncated to 28 chars) |
| `{{instance.namespace}}` | Instance namespace |
| `{{package.metadata.name}}` | Package name |

### Instance fields in a Test

```yaml
instance:
  name: my-app
  namespace: test-ns
  options:                       # package option overrides
    common_name: custom.host
  tenant: my-custom-tenant       # (tenant packages) injects vynil.solidite.fr/tenant label
  nodes:                         # injects Node mocks → cluster.ha = nodes.len() > 1
    - master01
    - worker01
  agent_yaml: agent-ha.yaml      # path relative to tests/, used as the cluster's agent.yaml
```

#### `nodes` — simulating HA via node count

`nodes` injects Node objects into the k8s mock layer. `build_context.rhai` derives
`cluster.ha = nodes.len() > 1` naturally, exactly as in production:

```yaml
instance:
  nodes: [master01, worker01]   # → cluster.ha = true
```

#### `agent_yaml` — explicit cluster configuration override

`agent_yaml` points to a YAML file in `tests/` (path relative to the package directory).
That file is used as the cluster's `agent.yaml` for this test, allowing you to explicitly
define `ha`, `prefered_storage`, or any property the operator would have set in the real
`agent.yaml`:

```yaml
instance:
  agent_yaml: agent-ha.yaml   # tests/agent-ha.yaml
```

```yaml
# tests/agent-ha.yaml
ha: true
prefered_storage: rook-cephfs
```

Values in `agent_yaml` take precedence over those derived from k8s mocks (same behavior
as `build_context.rhai`: keys present in `agent.yaml` are not recalculated). The two
mechanisms are complementary:

| Mechanism | What it tests | Corresponding prod path |
|---|---|---|
| `nodes` | HA derived from node count | Cluster without explicit `ha:` in `agent.yaml` |
| `agent_yaml` (with `ha: true`) | Explicitly configured HA | Cluster with `ha: true` in `agent.yaml` |

### Best practices

- **Predictable tests**: do not assert on fields that depend on upstream versions
  (container image tags, `app_version`) — they would break on every
  `agent package update`. Assert instead on structure, names, and values derived from options.
- **One test set per posture**: a minimal `default.yaml`, then dedicated sets for
  significant variants (HA enabled, major option activated…), rather than a single test
  that mixes everything.
- **Use context placeholders** (`{{instance.appslug}}`, `{{values.xxx}}`,
  `{{cluster.ha}}`) in expected values, so tests remain valid regardless of the
  instance name or simulated cluster configuration.
- **`nodes` vs `agent_yaml`**: use `nodes` to test the "HA inferred from nodes" path
  (no explicit `ha:`), and `agent_yaml` to test the "manually configured HA" path. Both
  can coexist in the same test, with `agent_yaml` taking precedence.

## Internal regression tests (`agent/tests/rhai_*.rs`)

The Rust suite runs the agent's Rhai scripts (not a package's) with a real engine
and mocks. It serves as a safety net during Rhai version upgrades, capturing semantic
changes in string and collection manipulation functions (`.filter()`, `.replace()`,
`.reduce()`, closures).

- **unit level**: isolated functions from `agent/scripts/lib/` (storage_class, wait,
  install_from_dir, gen_package, backup_context, resolv_service…);
- **integration level**: end-to-end service/install and service/delete lifecycle flows
  with K8s mocks, validating that the `lib/` scripts assemble correctly.

```bash
# run the full suite
cargo test -p agent

# a test family
cargo test -p agent --test rhai_build
```

## In CI

Recommended sequence for a package:

```bash
agent package lint -p ./pkg --format junit --junit-output-filename lint.xml
agent package test -p ./pkg --all --format json --junit-output-filename test.xml
agent package build -p ./pkg --signing-key "$SIGNING_KEY"   # signed publication
```
