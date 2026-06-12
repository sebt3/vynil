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

> Le framework de test fait l'objet d'améliorations en cours (variables de contexte
> d'assertions, surcharge cluster/tenant) — voir les issues du dépôt si un comportement
> attendu manque.

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
