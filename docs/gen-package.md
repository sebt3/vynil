# Système de génération et mise à jour des packages Vynil

## Vue d'ensemble

Un package Vynil est un répertoire contenant :
- `package.yaml` — manifeste décrivant le package (métadonnées, images, resources, options, dépendances)
- Des sous-répertoires de templates Handlebars (`.yaml.hbs`) et YAML statiques
- Des scripts Rhai (`scripts/`) pour les hooks du cycle de vie

La bibliothèque `gen_package.rhai` génère ces templates à partir d'un rendu Helm ou d'un manifeste Kubernetes brut. Le script `update.rhai` met à jour les tags d'images en interrogeant les registries OCI.

---

## Structure de `package.yaml`

```yaml
apiVersion: vinyl.solidite.fr/v1beta1
kind: Package
metadata:
  name: traefik             # identifiant du package (appslug dans les templates)
  category: networking      # catégorie libre
  type: system              # "system" | "tenant" | "service"
  app_version: "3.7.1"      # version de l'application (semver recommandé)
  description: >
    Traefik ingress controller.
  features:
    - upgrade
    - auto_config
images:
  traefik:                  # clé arbitraire, référencée par {{image_from_ctx this "traefik"}}
    registry: ghcr.io
    repository: traefik/traefik
    tag: v3.7.1             # mis à jour automatiquement par update.rhai
  acme-solver:
    registry: quay.io
    repository: traefik/acme-solver
    tag: v3.7.1
resources:                  # requests/limits par conteneur, référencées par {{resources_from_ctx}}
  traefik:
    requests:
      cpu: 100m
      memory: 128Mi
    limits:
      cpu: 1000m
      memory: 256Mi
requirements: []            # liste de dépendances (autres packages)
options:                    # schéma OpenAPI des options configurables
  replicas:
    type: integer
    default: 1
    description: Nombre de réplicas
```

### Règles sur `package.yaml`

- L'ordre des clés est **préservé** (manipulation texte brut, jamais re-parsé par serde_yaml qui trie alphabétiquement).
- Toujours commencer par `---`.
- Les sections `images:` et `resources:` sont écrasées à chaque regénération avec les valeurs extraites des manifestes.

---

## Génération des templates

### `placeholder()`

Génère une chaîne kubernetes-valide unique (`v<8-hex-chars>`, ex: `vf3a27b8c`).

**Pourquoi l'utiliser pour TOUT :** Helm insère le nom du release ET le namespace dans les noms de ressources (ex: `traefik-vynil-apps-clusterrole`). Si ces valeurs sont des chaînes réelles (comme `"traefik"` ou `"vynil-apps"`), elles peuvent apparaître dans d'autres contextes (noms d'images, labels…) et provoquer des faux positifs lors du remplacement. Une chaîne aléatoire distinctive évite ce problème.

```rhai
import "gen_package" as gen;

let name = gen::placeholder();   // remplacé par {{instance.appslug}} dans les templates
let ns   = gen::placeholder();   // remplacé par {{instance.namespace}} dans les templates

// En Rhai, pas de continuation \  —  utiliser += pour la lisibilité
let cmd  = `helm template ${name}`;
cmd     += " oci://ghcr.io/traefik/helm/traefik";
cmd     += ` --namespace=${ns}`;
cmd     += " --values values.yml 2>&1";

gen::gen_system(args.source, yaml_decode_multi(shell_output(cmd)), name, ns);
```

---

### `gen_system(path, docs[, name[, ns]])`

Génère les templates pour un **package système** (ressources cluster-wide : CRD, ClusterRole, Deployment dans un namespace dédié).

| Paramètre | Type | Description |
|-----------|------|-------------|
| `path` | `string` | Chemin du répertoire du package |
| `docs` | `array` | Liste de maps Kubernetes (sortie de `yaml_decode_multi`) |
| `name` | `string` | Nom du release Helm. Toutes ses occurrences → `{{instance.appslug}}`. Déduit de `package.yaml` si absent |
| `ns` | `string` | *(optionnel)* Namespace Helm (`--namespace=`). Toutes ses occurrences → `{{instance.namespace}}` |

**Répertoires créés :**
- `get_crds/` — CustomResourceDefinitions (`.yaml` statique ou `.yaml.hbs` si webhook de conversion)
- `get_systems/` — toutes les autres ressources (Deployment, ClusterRole, Service, etc.)

**Transformations appliquées :**
- `metadata.namespace` supprimé
- `metadata.labels` supprimés (remplacés par le contexte Vynil)
- Annotations Helm (`helm.sh/chart`, `meta.helm.sh/release-*`, `checksum/*`) supprimées
- Nom des ressources : `name` → `{{instance.appslug}}` ; pour ClusterRole/ClusterRoleBinding/Webhook : `name` → `{{instance.namespace}}-{{instance.appslug}}`
- Subjects des bindings : namespace → `{{instance.namespace}}`
- Images des conteneurs extraites vers `package.yaml` et remplacées par `{{image_from_ctx this "key"}}`
- Resources des conteneurs extraites vers `package.yaml` et remplacées par `{{json_to_str (resources_from_ctx this "key")}}`
- Selectors remplacés par `{{json_to_str (selector_from_ctx this comp="...")}}`
- SecurityContext ajouté si absent (`runAsNonRoot`, `readOnlyRootFilesystem`, capabilities drop ALL)
- `podAntiAffinity` converti en `topologySpreadConstraints`
- Annotations Reloader ajoutées (`configmap.reloader.stakater.com/reload`, etc.)
- Variables d'environnement plain-value extraites dans un ConfigMap dédié

**Exemple :**

```rhai
import "gen_package" as gen;

fn run(args) {
    let yaml          = yaml_decode(file_read(args.source + "/package.yaml"));
    let chart_version = yaml["metadata"]["app_version"];
    let name          = gen::placeholder();
    let ns            = gen::placeholder();

    let cmd  = `helm template ${name}`;
    cmd     += " oci://ghcr.io/traefik/helm/traefik";
    cmd     += " --include-crds";
    cmd     += ` --version ${chart_version}`;
    cmd     += ` --namespace=${ns}`;
    cmd     += ` -a "monitoring.coreos.com/v1/ServiceMonitor"`;
    cmd     += ` --values ${args.source}/values.yml 2>&1`;
    let out  = shell_output(cmd);

    gen::gen_system(args.source, yaml_decode_multi(out), name, ns);
}
```

---

### `gen_tenant(path, docs[, name[, ns]])`

Génère les templates pour un **package tenant** (ressources par namespace : Deployment, Service, PVC, etc.).

| Paramètre | Type | Description |
|-----------|------|-------------|
| `path` | `string` | Chemin du répertoire du package |
| `docs` | `array` | Liste de maps Kubernetes |
| `name` | `string` | Nom du release. Occurrences → `{{instance.appslug}}`. Déduit de `package.yaml` si absent |
| `ns` | `string` | *(optionnel)* Namespace Helm. Occurrences → `{{instance.namespace}}` |

**Répertoires créés :**
- `get_vitals/` — PersistentVolumeClaim
- `get_scalables/` — Deployment, StatefulSet, DaemonSet, ReplicaSet
- `get_systems/` — ressources cluster-wide (ClusterRole, CRD, Namespace…)
- `get_others/` — tout le reste (Service, ConfigMap, Role, Ingress, Certificate…)

**Exemple :**

```rhai
import "gen_package" as gen;

fn run(args) {
    let yaml          = yaml_decode(file_read(args.source + "/package.yaml"));
    let chart_version = yaml["metadata"]["app_version"];
    let name          = gen::placeholder();
    let ns            = gen::placeholder();

    let cmd  = `helm template ${name}`;
    cmd     += " oci://registry-1.docker.io/bitnamicharts/minio";
    cmd     += ` --version ${chart_version}`;
    cmd     += ` --namespace=${ns}`;
    cmd     += ` --values ${args.source}/values.yml 2>&1`;
    let out  = shell_output(cmd);

    gen::gen_tenant(args.source, yaml_decode_multi(out), name, ns);
}
```

---

### `gen_service(path, docs[, name[, ns]])`

Génère les templates pour un **package service** (tenant + CRDs propres). Identique à `gen_tenant` mais crée aussi `get_crds/`.

**Exemple :**

```rhai
import "gen_package" as gen;

fn run(args) {
    let yaml          = yaml_decode(file_read(args.source + "/package.yaml"));
    let chart_version = yaml["metadata"]["app_version"];
    let name          = gen::placeholder();
    let ns            = gen::placeholder();

    let cmd  = `helm template ${name}`;
    cmd     += " oci://ghcr.io/cert-manager/charts/cert-manager";
    cmd     += " --include-crds";
    cmd     += ` --version ${chart_version}`;
    cmd     += ` --namespace=${ns}`;
    cmd     += ` --values ${args.source}/values.yml 2>&1`;
    let out  = shell_output(cmd);

    gen::gen_service(args.source, yaml_decode_multi(out), name, ns);
}
```

---

## Extraction automatique

### Images → `image_from_ctx this "key"`

Lors de la génération, chaque `image:` de conteneur est parsé et décomposé :

```
ghcr.io/traefik/traefik:v3.7.1
  → registry:    ghcr.io
    repository:  traefik/traefik
    tag:         v3.7.1
```

La clé dans `package.yaml[images]` est construite à partir du nom du conteneur. Pour les `initContainers`, le préfixe `init-` est ajouté. Les arguments `--*-image=<img>` sont également extraits (ex: `--acme-http01-solver-image=quay.io/jetstack/acmesolver:v1.16.3`).

Dans les templates générés :
```yaml
image: {{image_from_ctx this "traefik"}}
# → ghcr.io/traefik/traefik:v3.7.1  (résolu au moment du déploiement)
```

### Resources → `resources_from_ctx this "key"`

```yaml
resources: {{json_to_str (resources_from_ctx this "traefik")}}
# → {"requests":{"cpu":"100m","memory":"128Mi"},"limits":{"cpu":"1000m","memory":"256Mi"}}
```

Si un conteneur n'a pas de `resources:` défini, l'entrée existante dans `package.yaml` est conservée.

### Selectors → `selector_from_ctx this comp="..."`

Les `matchLabels` des selectors et des `topologySpreadConstraints` sont remplacés par un helper qui génère des labels cohérents avec le contexte de l'instance :

```yaml
selector:
  matchLabels: {{json_to_str (selector_from_ctx this comp="controller")}}
```

### Labels de pod → `labels_from_ctx this`

Les labels du pod template (nécessaires pour que les selectors fonctionnent) :

```yaml
template:
  metadata:
    labels: {{json_to_str (labels_from_ctx this)}}
```

---

## Cycle de mise à jour (`update.rhai`)

Ce script est exécuté par la commande `agent package update`. Il :

1. Lit `package.yaml` avec `yaml_decode_ordered` (préservation de l'ordre des clés)
2. Pour chaque entrée dans `images:`, interroge le registry OCI pour lister les tags semver disponibles
3. Compare le tag courant avec le plus récent trouvé
4. Si un tag plus récent existe, met à jour `package.yaml` et écrit avec `yaml_encode_ordered`
5. Appelle `update_pre` (hook optionnel) avant et `update_post` (hook optionnel) après

### Hook `update_post.rhai`

Placé dans `scripts/update_post.rhai`, ce script regénère les templates après mise à jour des tags. C'est là qu'on appelle `gen_system`, `gen_tenant` ou `gen_service`.

**Exemple typique :**

```rhai
import "gen_package" as gen;

fn run(args) {
    let yaml          = yaml_decode(file_read(args.source + "/package.yaml"));
    let chart_version = yaml["metadata"]["app_version"];
    let name          = gen::placeholder();
    let ns            = gen::placeholder();

    let cmd  = `helm template ${name}`;
    cmd     += " oci://ghcr.io/traefik/helm/traefik";
    cmd     += " --include-crds";
    cmd     += ` --version ${chart_version}`;
    cmd     += ` --namespace=${ns}`;
    cmd     += ` -a "monitoring.coreos.com/v1/ServiceMonitor"`;
    cmd     += ` --values ${args.source}/values.yml 2>&1`;

    gen::gen_system(args.source, yaml_decode_multi(shell_output(cmd)), name, ns);
}
```

**Exemple avec filtrage des versions disponibles (ArtifactHub) :**

```rhai
import "gen_package" as gen;

fn run(args) {
    let hub           = new_http_client("https://artifacthub.io/api/v1");
    let pck           = json_decode(hub.get("packages/helm/traefik/traefik").body);
    let yaml          = yaml_decode(file_read(args.source + "/package.yaml"));
    let chart_version = yaml["metadata"]["app_version"];

    // Lister les versions disponibles dans la même major
    let more = pck.available_versions
        .map(|v| v.version)
        .filter(|v| parse_int(v.split(".")[0]) >= parse_int(chart_version.split(".")[0]));
    if more.len > 0 {
        print(`Versions disponibles à partir de ${chart_version}: ${yaml_encode(more)}`);
    }

    let name = gen::placeholder();
    let ns   = gen::placeholder();

    let cmd  = `helm template ${name}`;
    cmd     += " oci://ghcr.io/traefik/helm/traefik";
    cmd     += " --include-crds";
    cmd     += ` --version ${chart_version}`;
    cmd     += ` --namespace=${ns}`;
    cmd     += ` -a "monitoring.coreos.com/v1/ServiceMonitor"`;
    cmd     += ` --values ${args.source}/values.yml 2>&1`;

    gen::gen_system(args.source, yaml_decode_multi(shell_output(cmd)), name, ns);
}
```

---

## Helpers HBS disponibles dans les templates

| Helper | Signature | Description |
|--------|-----------|-------------|
| `image_from_ctx` | `(ctx "key")` | Rendu `registry/repository:tag` depuis `package.yaml[images][key]` |
| `resources_from_ctx` | `(ctx "key")` | Objet `{requests:{...}, limits:{...}}` depuis `package.yaml[resources][key]` |
| `selector_from_ctx` | `(ctx comp="key")` | Labels de selector pour le composant `key` |
| `labels_from_ctx` | `(ctx)` | Labels complets pour le pod template |
| `json_to_str` | `(value)` | Sérialise un objet en JSON inline (pour YAML scalaire) |
| `ctx_have_crd` | `(ctx "group/version/kind")` | Vrai si le CRD est installé dans le cluster |

**Exemple d'utilisation combinée :**

```yaml
spec:
  selector:
    matchLabels: {{json_to_str (selector_from_ctx this comp="worker")}}
  template:
    metadata:
      labels: {{json_to_str (labels_from_ctx this)}}
    spec:
      containers:
      - name: worker
        image: {{image_from_ctx this "worker"}}
        resources: {{json_to_str (resources_from_ctx this "worker")}}
```

**Ressource conditionnelle sur présence d'un CRD :**

```yaml
{{#if (ctx_have_crd this "servicemonitors.monitoring.coreos.com")}}
---
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
...
{{/if}}
```

---

## Règles de génération

1. **Utiliser `placeholder()` pour tout** — passer un placeholder aléatoire comme nom de release Helm ET comme namespace. Les chaînes réelles (comme `"traefik"` ou `"vynil-apps"`) peuvent apparaître dans d'autres contextes et provoquer des remplacements non désirés.

2. **Ordre des clés YAML** — `package.yaml` est modifié par manipulation de texte brut ; les sections `images:` et `resources:` sont remplacées ligne par ligne sans re-parser le fichier (évite la corruption par `rust_yaml` RoundTrip avec les block scalars `>`, `|`, `>-`, `|-`).

3. **Re-génération** — les sections `images:` et `resources:` sont **toujours écrasées**. Exception : si un conteneur n'a pas de `resources:` défini, l'entrée existante dans `package.yaml` est conservée.

4. **CRDs** — le fichier généré commence toujours par `# yamllint disable rule:line-length` car les CRDs contiennent des lignes très longues (descriptions OpenAPI).
