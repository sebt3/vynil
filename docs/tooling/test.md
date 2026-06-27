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

> The test framework is subject to ongoing improvements (assertion context variables,
> cluster/tenant overrides) — see the repository issues if an expected behavior is missing.

### Best practices

- **Predictable tests**: do not assert on fields that depend on upstream versions
  (container image tags, `app_version`) — they would break on every
  `agent package update`. Assert instead on structure, names, and values derived from options.
- **One test set per posture**: a minimal `default.yaml`, then dedicated sets for
  significant variants (HA enabled, major option activated…), rather than a single test
  that mixes everything.
- **Use context placeholders** (`{{instance.appslug}}`,
  `{{instance.namespace}}`) in expected values, so tests remain valid regardless of the
  instance name.

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
