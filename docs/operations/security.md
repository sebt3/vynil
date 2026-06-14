# Security & Threat Model

This page describes Vynil's actual security model today, its implications, and identified
improvement areas. Read before any production deployment.

## Threat model in one sentence

> **Installing a package = running arbitrary code with cluster-admin rights.**

Therefore: **only install packages from trusted JukeBox sources**, and treat the right to
create a `*Instance` as equivalent to cluster-admin access.

## Why

### The agent runs as cluster-admin

The `vynil-agent` ServiceAccount is bound to `cluster-admin` **and** to a custom
ClusterRole `*/*/*`
([`box/vynil/systems/rbac.yaml.hbs`](../../../box/vynil/systems/rbac.yaml.hbs)). The
bootstrap (`vynil-bootstrap`) also has a ClusterRole `*/*/*`
([`deploy/bootstrap/bootstrap.yaml`](../../../deploy/bootstrap/bootstrap.yaml)).

### The agent runs package code

The agent executes the Rhai scripts embedded in the package's OCI image. The «core» Rhai
engine exposes powerful primitives to **all** packages
([`common/src/shellhandler.rs`](../../../common/src/shellhandler.rs),
[`common/src/rhaihandler.rs`](../../../common/src/rhaihandler.rs)) :

- `shell_run` / `shell_output` — arbitrary shell execution (`sh -c …`);
- `get_env` — reading the agent's environment variables;
- `file_read` / `file_write` / `file_copy` / `create_dir` — filesystem access.

Combined with the above, a malicious or compromised package (or a compromised JukeBox/registry)
gains full cluster control: reading all secrets, creating privileged pods, exfiltration,
persistence.

### Image signatures are not verified at install time

Package images are **signed on push** (Cosign — see
[Build & signing](../build-signing.md)), but **no signature verification** is done on
pull/scan/install ([`common/src/ocihandler.rs`](../../../common/src/ocihandler.rs):
`pull_image`, `verify_tag_in_registry`). Signing therefore provides no provenance guarantee
on the consumer side today.

## What is correct

- **Password generation**: `gen_password` / `gen_password_alphanum` use a CSPRNG (`rand`)
  with weighted character sets.
- **HTTP**: the HTTP client uses `rustls`.
- **Registry auth**: credentials are read from `dockerconfigjson` Secrets and are not
  logged.
- **SecurityContext**: agent Jobs run as `runAsUser/Group 65534` (nobody), `fsGroup 65534`;
  package generation adds `runAsNonRoot`, `readOnlyRootFilesystem` and `drop ALL` to
  application containers.

## Improvement areas (tracked)

| Topic | Issue |
|---|---|
| Reduce agent privileges (least privilege per package type, restrict `shell_run`/`get_env` to trusted packages) | [#13](https://git.kydah.fr/shuss/vynil/issues/13) |
| Verify Cosign signature on pull/install, pin by digest, trust key per JukeBox | [#14](https://git.kydah.fr/shuss/vynil/issues/14) |

## Operational recommendations

1. **Restrict instance creation** via RBAC: only trusted operators should be able to create
   `SystemInstance`/`ServiceInstance`/`TenantInstance`.
2. **Trusted sources only**: limit JukeBoxes to registries you control; prefer private
   registries with `pull_secret`.
3. **Third-party package review**: audit Rhai scripts (and use of `shell_run`) before
   adding a category/package to a production JukeBox.
4. **Defense in depth**: while awaiting in-cluster signature verification, use an admission
   policy (Kyverno, Sigstore Policy Controller) with `cosign.pub` to require signed images.
5. **Network isolation**: restrict the agent's outbound access to the strict minimum
   (registry, backup S3).
