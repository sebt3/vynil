# vynil-core — Plan d'implémentation (T0)

Source de vérité : `docs/conception/vynil-core.md` (haut-niveau) + `.tasks/vynil-core/00_specs.md` (technique).

**Invariant cardinal** : le workspace complet (`cargo build` + `cargo test`) reste **vert après
chaque tâche**. Aucun fichier de `agent/`, `operator/`, `server/`, `kubectl-vynil/` n'est modifié
(zéro churn — garanti par les re-exports de `common`).

## Branche git

À créer une fois le plan validé, avant la tâche 01 :
`git switch -c feat/vynil-core origin/main`.

## Découpage

| # | Tâche | Fichier | Touche | Sortie |
|---|-------|---------|--------|--------|
| 01 | Scaffold `core/` + déplacement des modules feuilles (sans split interne) + `Error` core + ré-exports | `task-01-scaffold_et_modules_feuilles.md` | `core/` (neuf), `common/Cargo.toml`, `common/src/lib.rs` | workspace vert |
| 02 | `Script` (engine.rs) + `HandleBars` (hbs.rs) dans core ; split `yaml` ; newtypes + 4 constructeurs côté common | `task-02-engine_hbs_et_newtypes.md` | `core/`, `common/src/{rhaihandler,handlebarshandler,yamlhandler}.rs`, `common/src/lib.rs` | workspace vert + matrice features |
| 03 | Split `k8smock` (3 voies) : mocks k8s génériques → `core/k8s_mock.rs`, OCI mock → `core/oci_mock.rs`, instances/jukebox → common | `task-03-split_k8smock.md` | `core/`, `common/src/k8smock.rs`, `common/src/lib.rs` | workspace vert + matrice features |
| 04 | Documentation : `README.md`, `docs/architecture.md`, réconciliation de `docs/conception/vynil-core.md` (section Infrastructure → T2) | `task-04-documentation.md` | docs | — |

> Note : `k8smock` est isolé en tâche 03 (indépendante de 02). Son `k8smock_rhai_register`
> interleave enregistrement générique et vynil dans une seule fonction (~200 lignes), et les
> types génériques (`K8sObjectMock`/`K8sRawMock`/`K8sWorkloadMock`/`K8sGenericMock`) pèsent
> ~600 lignes — c'est l'extraction la plus délicate, elle mérite son propre passage.

## Détail des frontières par tâche

### Tâche 01 — feuilles (aucun split interne)
Déplacés tels quels (renommés) vers `core/` : `chrono`, `hashes`, `password`, `key`(ed25519),
`semver`, `glob`, `shell`, `http`, `http_mock`, `oci`(feat), `s3`(feat),
`k8s`(fusion k8sgeneric+raw+workload, feat). `core::Error/Result/RhaiRes/rhai_err` créés.
`common` : ajout dep `vynil-core`, ré-exports alias, `common::Error += Core(#[from] vynil_core::Error)`.
**Restent intacts dans common pour l'instant** : `rhaihandler`, `handlebarshandler`, `yamlhandler`,
`k8smock` (ils appellent les registres déplacés via les alias de ré-export).

### Tâche 02 — Script / HandleBars / yaml
- `engine.rs` : `Script` + `new_bare` + `core_common_rhai_register` (sans `vynil_owner`,
  **sans** `handlebars_rhai_register`).
- `hbs.rs` : `HandleBars` générique + `engine_mut()` + `CORE_HBS_HELPERS`.
- split `yaml` (serde→`core/yaml.rs`, `YamlDoc`/ordered→common).
- common : newtypes `Script`/`HandleBars` + `Deref`, 4 constructeurs, `vynil_owner_register`,
  `yaml_ordered_rhai_register`, **`handlebars_rhai_register` (bind `common::HandleBars`)**,
  7 helpers contextuels + `render_*`, `NATIVE_HBS_HELPERS` étendu.
- `k8smock` **reste intact** dans common (les constructeurs `new`/`new_mock` continuent
  d'appeler `crate::k8smock::{k8smock_rhai_register, oci_mock_rhai_register}`).
- migration des `mod tests` génériques selon §9 ; validation matrice de features.

### Tâche 03 — split k8smock (3 voies)
- **→ `core/k8s_mock.rs`** `#[cfg(k8s)]` : `K8sObjectMock`, `K8sRawMock`, `K8sWorkloadMock`,
  `K8sGenericMock` + helpers (`find_workload_mock`, `deep_merge_dynamic`, `merge_with_existing`,
  `upsert_in_list`) + un `k8s_mock_rhai_register` **générique** (DynamicObject + object/generic/raw/
  workload). Le stub mort `update_cache()` (k8smock.rs:9, jamais appelé) est **supprimé**.
- **→ `core/oci_mock.rs`** `#[cfg(oci)]` : `OciRegistryMock` + `oci_mock_rhai_register`.
- **→ reste common** : `K8sInstanceMock(Obj)`, `K8sJukeBoxMock` + `find/list_instance_mocks`,
  `find/list_jukebox_mocks`, `register_instance_common/children`. `common::k8smock_rhai_register`
  appelle `vynil_core::k8s_mock_rhai_register` (générique, mêmes `arced_mocks`/`created`) puis
  enregistre les parties instance/jukebox.
- validation matrice de features.

### Tâche 04 — doc
README (tableau features, exemple minimal), `docs/architecture.md` (mention de la crate core),
et alignement de la section « Infrastructure » de la conception sur le phasage T0/T2.

## Statut

- [x] 01 — scaffold + modules feuilles
- [x] 01b — fix matrice de features oci/s3
- [x] 02 — engine/hbs/yaml + newtypes
- [x] 03 — split k8smock (3 voies)
- [ ] 04 — documentation
