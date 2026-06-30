# Tests de paquet

Vynil distingue deux niveaux de test : les **tests d'un paquet** (`agent package test`,
écrits par l'auteur du paquet) et les **tests de régression internes** (suite Rust de
l'agent, garantissant la stabilité du moteur Rhai).

## Tests d'un paquet — `agent package test`

Exécute des scénarios définis dans `<package-dir>/tests/` avec un moteur Rhai réel et des
**mocks K8s/HTTP/OCI** : aucun cluster requis. Le rendu des templates et les hooks du cycle
de vie sont exécutés, et des assertions valident le résultat.

```bash
# tous les tests
agent package test -p ./my-package --all

# un test précis, avec dump du rendu
agent package test -p ./my-package --test-name install-default \
  --template-output-filename rendered.yaml

# rapport JUnit pour la CI
agent package test -p ./my-package --all --format json \
  --junit-output-filename results.xml
```

| Option | Rôle |
|---|---|
| `--test-name <name>` | Exécute un seul test. |
| `--all` | Exécute tous les tests. |
| `--testsets-dir <dir>` | Répertoire additionnel de jeux de tests. |
| `--format text\|json` | Format de sortie. |
| `--junit-output-filename <file>` | Rapport JUnit XML. |
| `--template-output-filename <file>` | Dump du rendu (test unique uniquement). |

Le répertoire `tests/` doit exister, sinon l'agent renvoie `MissingTestDirectory`.

### Ce qu'un test peut faire

- fournir des **options** d'instance et un **contexte** (cluster/tenant) ;
- déclarer des **mocks** : objets K8s présents, réponses HTTP, manifestes OCI ;
- exécuter un flow (`context → install`, `context → delete`, …) ;
- **asserter** sur les objets créés, le statut, ou le rendu des templates.

### Variables disponibles dans les sélecteurs d'assertions

Les sélecteurs (`selector.name`, `selector.namespace`) et les valeurs attendues (`value`) sont
des templates Handlebars résolus avec le **contexte réel** produit par `ctx::run()` — le même
que celui qu'utilisent les scripts Rhai et les templates du paquet. Toutes les clés du contexte
sont disponibles, notamment :

| Variable | Description |
|---|---|
| `{{values.xxx}}` | Options de l'instance fusionnées avec les défauts du `package.yaml` |
| `{{defaults.xxx}}` | Valeurs par défaut brutes du `package.yaml` |
| `{{cluster.ha}}` | `true` si le cluster a plus d'un nœud |
| `{{cluster.storage_classes}}` | Liste des StorageClass mockées |
| `{{tenant.name}}` | Nom du tenant (paquets de type `tenant` uniquement) |
| `{{tenant.namespaces}}` | Namespaces du tenant |
| `{{instance.appslug}}` | Slug de l'instance (`{instance}-{package}`, tronqué à 28 caractères) |
| `{{instance.namespace}}` | Namespace de l'instance |
| `{{package.metadata.name}}` | Nom du paquet |

### Champs de `instance` dans un Test

```yaml
instance:
  name: my-app
  namespace: test-ns
  options:                       # overrides des options du paquet
    common_name: custom.host
  tenant: my-custom-tenant       # (paquets tenant) injecte le label vynil.solidite.fr/tenant
  nodes:                         # injecte des mocks Node → cluster.ha = nodes.len() > 1
    - master01
    - worker01
  agent_yaml: agent-ha.yaml      # chemin relatif à tests/, utilisé comme agent.yaml du cluster
```

#### `nodes` — simuler le HA par décompte de nœuds

`nodes` injecte des objets Node dans la couche k8s mock. `build_context.rhai` dérive
`cluster.ha = nodes.len() > 1` naturellement, exactement comme en production :

```yaml
instance:
  nodes: [master01, worker01]   # → cluster.ha = true
```

#### `agent_yaml` — surcharger la configuration cluster explicite

`agent_yaml` pointe vers un fichier YAML dans `tests/` (chemin relatif au répertoire du paquet).
Ce fichier est utilisé comme `agent.yaml` du cluster pour ce test, permettant de définir
explicitement `ha`, `prefered_storage`, ou toute autre propriété que l'opérateur aurait
configurée dans le vrai `agent.yaml` :

```yaml
instance:
  agent_yaml: agent-ha.yaml   # tests/agent-ha.yaml
```

```yaml
# tests/agent-ha.yaml
ha: true
prefered_storage: rook-cephfs
```

Les valeurs dans `agent_yaml` ont la priorité sur celles dérivées des mocks k8s (même
comportement que `build_context.rhai` : les clés présentes dans `agent.yaml` ne sont pas
recalculées). Les deux mécanismes sont complémentaires :

| Mécanisme | Ce qu'il teste | Chemin prod correspondant |
|---|---|---|
| `nodes` | HA dérivé du comptage des nœuds | Cluster sans `ha:` explicite dans `agent.yaml` |
| `agent_yaml` (avec `ha: true`) | HA configuré explicitement | Cluster avec `ha: true` dans `agent.yaml` |

### Bonnes pratiques

- **Tests prévisibles** : ne pas asserter sur des champs dépendants de la version amont
  (tags d'images de conteneurs, `app_version`) — ils casseraient à chaque
  `agent package update`. Asserter plutôt sur la structure, les noms et les valeurs
  dérivées des options.
- **Un jeu de test par posture** : un `default.yaml` minimal, puis des jeux dédiés aux
  variantes significatives (HA activé, option majeure activée…), plutôt qu'un test unique
  qui mélange tout.
- **Utiliser les variables de contexte** (`{{instance.appslug}}`, `{{values.xxx}}`,
  `{{cluster.ha}}`) dans les sélecteurs d'assertions pour que les tests restent valides
  quel que soit le nom de l'instance ou la configuration du cluster simulé.
- **`nodes` vs `agent_yaml`** : utiliser `nodes` pour tester le chemin "HA déduit des
  nœuds" (sans `ha:` explicite), et `agent_yaml` pour tester le chemin "HA configuré
  manuellement". Les deux peuvent coexister dans le même test, `agent_yaml` ayant la priorité.

## Tests de régression internes (`agent/tests/rhai_*.rs`)

La suite Rust exécute les scripts Rhai *de l'agent* (pas ceux d'un paquet) avec un moteur
réel et des mocks. Elle sert de filet de sécurité lors des montées de version de Rhai, en
capturant les changements de sémantique des fonctions de manipulation de chaînes et de
collections (`.filter()`, `.replace()`, `.reduce()`, closures).

- **niveau unitaire** : fonctions isolées de `agent/scripts/lib/` (storage_class, wait,
  install_from_dir, gen_package, backup_context, resolv_service…) ;
- **niveau intégration** : flows de cycle de vie service/install et service/delete de bout
  en bout avec mocks K8s, validant que les scripts `lib/` s'assemblent correctement.

```bash
# lancer toute la suite
cargo test -p agent

# une famille de tests
cargo test -p agent --test rhai_build
```

## En CI

Enchaînement recommandé pour un paquet :

```bash
agent package lint -p ./pkg --format junit --junit-output-filename lint.xml
agent package test -p ./pkg --all --format json --junit-output-filename test.xml
agent package build -p ./pkg --signing-key "$SIGNING_KEY"   # publication signée
```
