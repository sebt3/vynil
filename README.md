
# Vynil
Vynil is an installer for kubernetes intended to be used either at home or for SaaS. The goal is to build a package manager akin to dpkg/rpm but for the kubernetes.

Unlike helm, kustomize, argoCD, Flux... which all give you all the flexibility to install as you please. Vynil main goal is to help create an integrated distribution for kubernetes, so customisation come scarse but integration of everything by default. Vynil differ also from openshift since olm can only install operators. Requiering an operator to manage an app while there is already a pseudo-generic installtion operator is madness. Olm should be able to install awx and phpmyadmin, but instead, you need a tower operator to install awx (as if the main use case is running many instances AWX instances). You even need an operator to install kube-virt while there can only be a single instance of kubevirt installed on a k8s cluster. Yet again, this design is madness... Redhat used to known how to install things properly /rant off

## Installation
```
kubectl create ns vynil-system
kubectl apply -k github.com/sebt3/vynil//deploy
```

## Package Tooling

### `agent package lint <package-dir>`

Analyzes a Vynil package statically without deployment. This command performs comprehensive linting of package structure, templates, and scripts.

**Options:**
- `-p / --package-dir` : Package directory (default: `/tmp/package`)
- `-c / --config-dir` : Directory for additional Rhai scripts (default: `.`)
- `--format` : Output format (default: `text`, options: `text`, `junit`)
- `--level` : Minimum severity level to display (default: `all`, options: `error`, `warn`, `all`)
- `--junit-output-filename` : Write JUnit XML report to file

**Checks Performed:**

**Package Structure (`package/`):**
- Validates `package.yaml` presence and format
- Checks required directory structure
- Verifies resource definitions

**Handlebars Templates (`hbs/`):**
- Syntax validation
- Unknown helper detection
- Unknown partial detection
- Context variable consistency with `package.yaml`
- Resource and image key validation
- Correct package type usage

**Rhai Scripts (`rhai/`):**
- Syntax validation
- Unresolved import detection
- Dead code detection
- Unused variable detection
- Shadowed variable detection
- Unused parameter detection
- Unused function detection
- API mode validation (no full-API in core scripts)
- Package type validation (no tenant access in system packages)
- Context hook return validation

**Configuration:**

Create a `.vynil-lint.yaml` file in the package root to customize linting behavior:

```yaml
disable:
  - rhai/unused-variable
  - hbs/unused-helper

override:
  rhai/unused-function: error
  hbs/unknown-helper: warn

files:
  - glob: "handlebars/helpers/**"
    disable:
      - rhai/unused-function
  - glob: "scripts/context_*.rhai"
    override:
      rhai/context-hook-no-return: error
```

**Inline Disable:**

Disable a specific rule on a single line:

- **Rhai:** `// vynil-lint-disable rhai/unused-variable`
- **Handlebars:** `{{!-- vynil-lint-disable hbs/unknown-helper --}}`

**Exit Codes:**
- `0` : No issues found
- `1` : Errors detected
- `2` : Only warnings detected
