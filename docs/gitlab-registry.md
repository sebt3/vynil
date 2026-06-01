# GitLab Container Registry — Guide d'intégration

## Architecture

La source `gitlab` permet à un JukeBox de scanner, push et pull des packages depuis un GitLab Container Registry (gitlab.com ou self-hosted).

```
JukeBox (spec.source.gitlab)
  ├── url      → instance GitLab (API REST v4)
  ├── registry → registry OCI (push/pull d'images)
  └── project  → chemin complet du projet (group/project)
```

Contrairement à Harbor où `registry` = host API = host OCI, GitLab dissocie les deux :
- **`url`** : sert exclusivement aux appels API (`/api/v4/...`) — ex: `https://gitlab.example.com`
- **`registry`** : sert au push/pull OCI — ex: `registry.gitlab.example.com`

---

## Tokens et stratégie d'authentification

Trois contextes, trois types de tokens.

### 1. Push depuis le Makefile (dev local)

Utiliser un **Personal Access Token (PAT)** avec le scope `write_registry`.

```makefile
REGISTRY_USER ?= my-gitlab-username
REGISTRY_PASS ?= $(GITLAB_PAT)
```

> ⚠️ Le PAT est lié à un compte utilisateur. S'il quitte l'organisation, le token est révoqué.
> Préférer un "project bot token" ou un "service account" quand c'est possible.

### 2. Push depuis la CI GitLab

Utiliser le **`CI_JOB_TOKEN`** injecté automatiquement par GitLab CI.

```yaml
build:
  script:
    - make push REGISTRY_USER=gitlab-ci-token REGISTRY_PASS=$CI_JOB_TOKEN
```

Le `CI_JOB_TOKEN` est scopé au job, n'expire pas à gérer et n'a pas besoin d'être stocké en secret.

> ⚠️ Par défaut, `CI_JOB_TOKEN` ne peut s'authentifier qu'au registry **du même projet**.
> Pour pusher dans un projet différent, activer le _CI/CD job token allowlist_ dans les settings du projet cible
> (`Settings → CI/CD → Token Access`).

### 3. Scan et pull dans le cluster (JukeBox / instances)

Utiliser un **Deploy Token** read-only stocké en secret Kubernetes de type `dockerconfigjson`.

#### Scopes requis

| Scope          | Utilité                                                  |
|----------------|----------------------------------------------------------|
| `read_registry` | Pull OCI des layers (ocihandler.rs)                     |
| `read_api`      | Appels API REST `/api/v4/.../registry/repositories`     |

> ⚠️ **Point d'attention** : `read_registry` seul ne suffit pas pour le scan.
> Le script `scan_gitlab.rhai` appelle l'API GitLab REST pour lister les repositories.
> Sans `read_api`, l'appel retourne HTTP 403.
> Ce scope est disponible sur les deploy tokens depuis **GitLab 13.9**.

#### Création du deploy token

```
GitLab → Project → Settings → Repository → Deploy tokens
  Name   : vynil-scan-pull
  Scopes : ✅ read_registry  ✅ read_api
```

#### Création du secret Kubernetes

```bash
kubectl create secret docker-registry gitlab-registry-pull \
  --namespace vynil-system \
  --docker-server=registry.gitlab.example.com \
  --docker-username=<deploy-token-name> \
  --docker-password=<deploy-token-value>
```

#### Exemple de JukeBox

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

## Fonctionnement du scan

Le script `agent/scripts/lib/scan_gitlab.rhai` :

1. Lit le `pull_secret` (dockerconfigjson) → extrait `user:pass`
2. Utilise `pass` comme Bearer token pour l'API REST GitLab
3. `GET {url}/api/v4/projects/{encoded_project}/registry/repositories?per_page=20`
   - Le project path est URL-encodé (`/` → `%2F`)
   - La pagination est pilotée par le header `X-Total-Pages`
4. Retourne la liste des `location` (ex: `registry.gitlab.example.com/my-group/my-project/my-image`)
5. Le scan principal (`scan.rhai`) prend le relais : OCI list_tags + lecture des annotations

---

## Fonctionnement du push (build.rhai)

Le push utilise `ocihandler.rs` via `new_registry(registry, user, pass)` — standard OCI, aucun code spécifique GitLab.

| Contexte   | user                | pass                  |
|------------|---------------------|-----------------------|
| Makefile   | nom d'utilisateur   | PAT (`write_registry`) |
| CI GitLab  | `gitlab-ci-token`   | `$CI_JOB_TOKEN`       |

---

## Fonctionnement du pull (ocihandler.rs)

Identique au pull Harbor : le `pull_secret` (dockerconfigjson) est lu, les credentials sont passés à `oci_client` pour l'authentification standard Docker. Aucun code spécifique GitLab.

---

## Points d'attention récapitulatifs

| # | Point                                | Détail                                                              |
|---|--------------------------------------|---------------------------------------------------------------------|
| 1 | Deploy token scope `read_api`        | Obligatoire pour le scan — `read_registry` seul ne suffit pas      |
| 2 | Deux URLs distinctes                 | `url` (API) ≠ `registry` (OCI) pour les instances self-hosted      |
| 3 | PAT lié à un user                    | Préférer project bot token en prod pour le Makefile                 |
| 4 | CI_JOB_TOKEN cross-project           | Nécessite l'activation du token allowlist sur le projet cible       |
| 5 | Encodage du project path             | `/` → `%2F` dans l'URL API — géré automatiquement par scan_gitlab.rhai |
| 6 | GitLab 13.9 minimum                  | Scope `read_api` sur deploy token disponible depuis cette version   |
