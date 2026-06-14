# GitLab Container Registry — Integration Guide

## Architecture

The `gitlab` source allows a JukeBox to scan, push, and pull packages from a GitLab Container Registry (gitlab.com or self-hosted).

```
JukeBox (spec.source.gitlab)
  ├── url      → GitLab instance (API REST v4)
  ├── registry → OCI registry (image push/pull)
  └── project  → full project path (group/project)
```

Unlike Harbor where `registry` = API host = OCI host, GitLab separates the two:
- **`url`**: used exclusively for API calls (`/api/v4/...`) — e.g. `https://gitlab.com`
- **`registry`**: used for OCI push/pull — e.g. `registry.gitlab.com`

---

## Tokens and authentication strategy

Three contexts, three token types.

### 1. Push from Makefile (local dev)

Use a **Personal Access Token (PAT)** with the `write_registry` scope.

```makefile
REGISTRY_USER ?= my-gitlab-username
REGISTRY_PASS ?= $(GITLAB_PAT)
```

> ⚠️ The PAT is tied to a user account. If they leave the organization, the token is revoked.
> Prefer a "project bot token" or a "service account" when possible.

### 2. Push from GitLab CI

Use the **`CI_JOB_TOKEN`** automatically injected by GitLab CI.

```yaml
build:
  script:
    - make push REGISTRY_USER=gitlab-ci-token REGISTRY_PASS=$CI_JOB_TOKEN
```

The `CI_JOB_TOKEN` is scoped to the job, has no expiry to manage, and does not need to be stored as a secret.

> ⚠️ By default, `CI_JOB_TOKEN` can only authenticate to the registry **of the same project**.
> To push to a different project, enable the _CI/CD job token allowlist_ in the target project settings
> (`Settings → CI/CD → Token Access`).

### 3. Scan and pull in the cluster (JukeBox / instances)

Use a **Project Access Token** (or Deploy Token) read-only, stored as a Kubernetes secret of type `dockerconfigjson`.

#### Required scopes

| Scope          | Use case                                                 |
|----------------|----------------------------------------------------------|
| `read_registry` | OCI layer pull (ocihandler.rs)                          |
| `read_api`      | REST API calls `/api/v4/.../registry/repositories`      |

> ⚠️ **Important**: `read_registry` alone is not sufficient for scanning.
> The `scan_gitlab.rhai` script calls the GitLab REST API to list repositories.
> Without `read_api`, the call returns HTTP 403.
> This scope has been available on deploy tokens since **GitLab 13.9**.

#### Minimum required role (Project Access Token)

The `/api/v4/projects/:id/registry/repositories` API requires at minimum the **Reporter** role.
A token created with the **Guest** role returns HTTP 403 even with the correct scopes.

> ⚠️ For **Project Access Tokens** (`glpat-…`), the role is chosen at creation time.
> The same restriction applies to **Deploy Tokens**: create with role ≥ Reporter.

#### Creating the project access token

```
GitLab → Project → Settings → Access Tokens
  Name   : vynil-scan-pull
  Role   : Reporter        ← minimum required
  Scopes : ✅ read_registry  ✅ read_api
```

#### Creating the Kubernetes secret

```bash
kubectl create secret docker-registry gitlab-registry-pull \
  --namespace vynil-system \
  --docker-server=registry.gitlab.example.com \
  --docker-username=<deploy-token-name> \
  --docker-password=<deploy-token-value>
```

#### JukeBox example

```yaml
apiVersion: vynil.solidite.fr/v1
kind: JukeBox
metadata:
  name: my-gitlab-catalog
spec:
  schedule: "0 * * * *"
  pull_secret: gitlab-registry-pull
  source:
    gitlab:
      url: https://gitlab.example.com
      registry: registry.gitlab.example.com
      project: my-group/my-project
```

---

## How scanning works

The `agent/scripts/lib/scan_gitlab.rhai` script:

1. Reads the `pull_secret` (dockerconfigjson) → extracts `user:pass`
2. Uses `pass` as the Bearer token for the GitLab REST API
3. Resolves the project path to a numeric ID via `GET {url}/api/v4/projects?search={name}`
   and filters on `path_with_namespace == project`
   > Note: URL-encoding (`/` → `%2F`) in the HTTP path is silently decoded by
   > the HTTP client (reqwest/url-crate), which resulted in HTTP 404. The numeric ID works around this bug.
4. `GET {url}/api/v4/projects/{id}/registry/repositories?per_page=20`
   - Pagination is driven by the `X-Total-Pages` header
5. Returns the list of `location` values (e.g. `registry.gitlab.com/my-group/my-project/my-image`)
6. The main scan (`scan.rhai`) takes over: OCI list_tags + reading annotations

---

## How push works (build.rhai)

Push uses `ocihandler.rs` via `new_registry(registry, user, pass)` — standard OCI, no GitLab-specific code.

| Context     | user                | pass                   |
|-------------|---------------------|------------------------|
| Makefile    | username            | PAT (`write_registry`) |
| GitLab CI   | `gitlab-ci-token`   | `$CI_JOB_TOKEN`        |

---

## How pull works (ocihandler.rs)

Identical to Harbor pull: the `pull_secret` (dockerconfigjson) is read, credentials are passed to `oci_client` for standard Docker authentication. No GitLab-specific code.

---

## Summary of key points

| # | Point                                | Detail                                                              |
|---|--------------------------------------|---------------------------------------------------------------------|
| 1 | `read_api` scope required            | Mandatory for scanning — `read_registry` alone returns HTTP 403     |
| 2 | **Reporter** role minimum            | A token with Guest role returns HTTP 403 even with correct scopes   |
| 3 | Two distinct URLs                    | `url` (API) ≠ `registry` (OCI) for self-hosted instances           |
| 4 | PAT tied to a user                   | Prefer project access token in prod for the Makefile               |
| 5 | CI_JOB_TOKEN cross-project           | Requires enabling the token allowlist on the target project         |
| 6 | Numeric ID vs encoded path           | `%2F` decoded by reqwest → scan uses the GitLab numeric ID         |
| 7 | GitLab 13.9 minimum                  | `read_api` scope on deploy token available since this version       |
