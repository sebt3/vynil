# Cycle de vie d'un paquet

L'agent exécute des scripts Rhai pour chaque opération (`install`, `delete`, `reconfigure`,
`backup`, `restore`). Cette page décrit l'orchestration, les phases et les points
d'extension. Les scripts de référence sont dans
[`agent/scripts/{system,service,tenant}/`](../../agent/scripts/).

## Orchestration générale

Chaque opération a un script chef d'orchestre (`install.rhai`, `delete.rhai`, …) qui :

1. exécute un hook `<op>_pre` optionnel ;
2. traite chaque **phase** présente (selon l'existence du répertoire correspondant) ;
3. recharge l'instance depuis l'API entre les phases (pour voir les statuts mis à jour) ;
4. exécute un hook `<op>_post` optionnel ;
5. met à jour le statut final (`set_status_ready`).

Le mécanisme `import_run(name, …)` importe et lance `name::run(...)` s'il existe, et ignore
silencieusement les modules/fonctions absents — c'est ce qui rend les phases et les hooks
optionnels.

## Phases d'installation

Ordre d'application (tenant — `agent/scripts/tenant/install.rhai`) :

| # | Phase | Répertoire | Contenu typique | Statut renseigné |
|---|---|---|---|---|
| 1 | befores | `befores/` | jobs d'init, secrets pré-requis | `status.befores` |
| 2 | vitals | `vitals/` | PVC, données persistantes | `status.vitals` |
| 3 | tofu | `tofu/` | ressources OpenTofu/Terraform | `status.tfstate` |
| — | init_from | — | restauration si 1er install + `initFrom` | — |
| 4 | others | `others/` | Service, ConfigMap, Ingress, Role… | `status.others` |
| 5 | scalables | `scalables/` | Deployment, StatefulSet… | `status.scalables` |
| 6 | posts | `posts/` | actions finales | `status.posts` |
| — | backup | — | `schedule_backup` ou `delete_backup` | — |

Après les phases, si l'instance a des `vitals` et que la sauvegarde est activée
(`use_backup`) et qu'un secret `backup-settings` existe, un `schedule_backup` est posé ;
sinon le backup éventuel est retiré.

## Phases de désinstallation

`delete.rhai` procède dans l'**ordre inverse** et s'appuie sur les listes du `status`
(et non sur le contenu du paquet) pour savoir quoi supprimer :

```text
posts → scalables → tofu → others (+ delete_backup) → vitals → befores
```

Chaque sous-script (`delete_others`, `delete_vitals`, …) itère sur `instance.status.<phase>`,
récupère chaque objet via `k8s_resource(...)`, le supprime et attend sa disparition
(`wait_deleted`, timeout 5 min). Les erreurs `NotFound` sont tolérées.

> C'est la raison pour laquelle le `status` est la source de vérité du nettoyage : même si
> le contenu du paquet change, la désinstallation sait quoi détruire tant qu'elle dispose
> du `status`. Voir la limite tenant→service dans [Dépannage](../operations/troubleshooting.md).

## Points d'extension (hooks)

Pour chaque phase et chaque opération, des hooks `*_pre`/`*_post` peuvent être fournis par
le paquet dans `scripts/` :

- `install_pre.rhai`, `install_post.rhai`
- `install_befores_pre.rhai`, `install_befores_post.rhai`, … (idem vitals/others/scalables/posts)
- `delete_pre.rhai`, `delete_post.rhai`, et les `delete_<phase>_pre/post`
- `context.rhai` / `context_tenant.rhai` / `context_service.rhai` — construisent le contexte
  d'exécution (variables disponibles aux templates et scripts)

Un hook reçoit `(instance, context[, args])` et peut renvoyer une `map` pour enrichir le
`context` retourné aux phases suivantes.

## Autres opérations

| Opération | Script | Rôle |
|---|---|---|
| `reconfigure` | `reconfigure.rhai` | Recalcule et réapplique sans tout réinstaller (suite à un changement d'options). |
| `backup` | `backup.rhai` + `backup_run.rhai` + `backup_prepare_*` | Sauvegarde Restic des vitals (PostgreSQL, MySQL, MongoDB, Redis, secrets). |
| `restore` | `restore.rhai` + `restore_run.rhai` + `restore_*` | Restauration depuis un snapshot. |
| `maintenance_start` / `maintenance_stop` | — | Met l'application en pause (scale down) pour les opérations de données. |

## Bibliothèque réutilisable

`agent/scripts/lib/` fournit des fonctions partagées :

- `gen_package.rhai` — génération des templates (voir [Génération](../gen-package.md))
- `secret_dockerconfigjson.rhai` — lecture des secrets imagePull
- `scan_harbor.rhai` — listing des dépôts Harbor
- `backup_context.rhai`, `wait.rhai`, `storage_class.rhai`, `resolv_service.rhai`,
  `install_from_dir.rhai`, `tofu_gen.rhai`, …

Ces fonctions sont couvertes par des tests de régression (voir [Tests de paquet](../tooling/test.md)
et la suite `agent/tests/rhai_*.rs`).
