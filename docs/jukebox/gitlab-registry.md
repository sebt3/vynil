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
- **`url`** : sert exclusivement aux appels API (`/api/v4/...`) — ex: `https://gitlab.com`
- **`registry`** : sert au push/pull OCI — ex: `registry.gitlab.com`

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

Utiliser un **Project Access Token** (ou Deploy Token) read-only stocké en secret Kubernetes de type `dockerconfigjson`.

#### Scopes requis

| Scope          | Utilité                                                  |
|----------------|----------------------------------------------------------|
| `read_registry` | Pull OCI des layers (ocihandler.rs)                     |
| `read_api`      | Appels API REST `/api/v4/.../registry/repositories`     |

> ⚠️ **Point d'attention** : `read_registry` seul ne suffit pas pour le scan.
> Le script `scan_gitlab.rhai` appelle l'API GitLab REST pour lister les repositories.
> Sans `read_api`, l'appel retourne HTTP 403.
> Ce scope est disponible sur les deploy tokens depuis **GitLab 13.9**.

#### Rôle minimum requis (Project Access Token)

L'API `/api/v4/projects/:id/registry/repositories` exige au minimum le rôle **Reporter**.
Un token créé avec le rôle **Guest** retourne HTTP 403 même avec les scopes corrects.

> ⚠️ Pour les **Project Access Tokens** (`glpat-…`), le rôle est choisi à la création.
> Pour les **Deploy Tokens**, la restriction s'applique aussi : créer avec rôle ≥ Reporter.

#### Création du project access token

```
GitLab → Project → Settings → Access Tokens
  Name   : vynil-scan-pull
  Role   : Reporter        ← minimum requis
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
3. Résout le project path en ID numérique via `GET {url}/api/v4/projects?search={name}`
   et filtre sur `path_with_namespace == project`
   > Note : l'URL-encoding (`/` → `%2F`) dans le path HTTP est silencieusement décodé par
   > le client HTTP (reqwest/url-crate), ce qui donnait HTTP 404. L'ID numérique contourne ce bug.
4. `GET {url}/api/v4/projects/{id}/registry/repositories?per_page=20`
   - La pagination est pilotée par le header `X-Total-Pages`
5. Retourne la liste des `location` (ex: `registry.gitlab.com/my-group/my-project/my-image`)
6. Le scan principal (`scan.rhai`) prend le relais : OCI list_tags + lecture des annotations

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
| 1 | Scope `read_api` requis              | Obligatoire pour le scan — `read_registry` seul retourne HTTP 403  |
| 2 | Rôle **Reporter** minimum            | Un token avec rôle Guest retourne HTTP 403 même avec les bons scopes |
| 3 | Deux URLs distinctes                 | `url` (API) ≠ `registry` (OCI) pour les instances self-hosted      |
| 4 | PAT lié à un user                    | Préférer project access token en prod pour le Makefile              |
| 5 | CI_JOB_TOKEN cross-project           | Nécessite l'activation du token allowlist sur le projet cible       |
| 6 | ID numérique vs path encodé          | `%2F` décodé par reqwest → scan utilise l'ID numérique GitLab      |
| 7 | GitLab 13.9 minimum                  | Scope `read_api` sur deploy token disponible depuis cette version   |
