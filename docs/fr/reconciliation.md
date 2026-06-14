# Réconciliation & cycle de vie

Cette page décrit ce que fait l'opérateur (le « quoi ») et ce que fait l'agent (le
« comment »). Le code générique de réconciliation des instances vit dans
[`operator/src/instance_common.rs`](../../operator/src/instance_common.rs) via le trait
`InstanceKind`, partagé par les trois types d'instance.

## Scan d'une JukeBox

```mermaid
flowchart TD
    JB[JukeBox] -->|cron schedule| CJ[CronJob]
    CJ --> J[Job : agent box scan]
    J --> SRC{Type de source}
    SRC -->|OCI : list/harbor/gitlab/script| OCI[Liste les tags du registre]
    SRC -->|http/s3| CACHE[Télécharge index.yaml + fichiers paquets]
    OCI --> F[Filtre : semver valide + maturité + version Vynil]
    CACHE --> F
    F --> WP[Calcule les waypoints d'upgrade\n1 par époque MinimumPreviousVersion]
    WP --> ST[Écrit JukeBox.status.packages]
```

L'opérateur ([`operator/src/jukebox.rs`](../../operator/src/jukebox.rs)) maintient le CronJob,
détecte la complétion du Job de scan (condition `Complete`/`Failed`), et ne recharge le
cache **qu'une fois par complétion** (suivi via l'annotation `last-scan-time`).

### Scan standalone (`box file-scan`)

```mermaid
flowchart LR
    SPEC[Spec JukeBox YAML locale] --> FS[agent box file-scan]
    FS --> OCI2[Scanne les registres OCI\nsans connexion Kubernetes]
    OCI2 --> WP2[Calcule les waypoints\npour les 3 niveaux de maturité]
    WP2 --> IDX["Produit index.yaml\n+ category_name.yaml"]
    IDX -->|upload optionnel| CACHE2[(Cache HTTP/S3)]
    CACHE2 --> JB2[JukeBox source http/s3]
    JB2 --> F2[Applique le filtre maturité\n+ recalcule les waypoints]
    F2 --> ST2[Met à jour status.packages]
```

## Réconciliation d'une instance (apply)

```mermaid
flowchart TD
    I[Instance CRD] --> V[current_version = status.tag]
    V --> SEL[Sélection du paquet dans le cache JukeBox]
    SEL -->|absent| ERR1[condition missing_package\n→ requeue 15 min]
    SEL -->|trouvé| REQ[Vérification des prérequis]
    REQ -->|échec| ERR2[condition missing_requirement\n→ requeue]
    REQ -->|ok| REC[Construction des recommandations]
    REC --> VS[Exécution du value_script Rhai]
    VS --> JOB[Rendu du Job template]
    JOB --> APPLY[Création/upsert du Job]
    APPLY --> RQ[Requeue 15 min]
```

`do_reconcile<T>()` :

1. `current_version = status.tag` (vide au premier install).
2. **Sélection du paquet** dans le cache de la JukeBox :
   - `name` + `category` + `usage == type de l'instance`,
   - `is_min_version_ok(current_version)` — chaîne d'upgrade respectée,
   - `is_vynil_version_ok()` — framework compatible.
   - Si absent → condition `missing_package` et requeue (15 min).
3. **Prérequis** (`check_requirements`) : CRDs, services système, ressources… Échec →
   condition `missing_requirement` et requeue.
4. **Recommandations** : listes optionnelles (CRDs présents, services système/tenant
   disponibles) injectées dans le contexte.
5. **value_script** Rhai (si présent) → variables de contrôle (`ctrl_values`).
6. **initFrom.version** (premier install) → vérification que le tag existe (cache puis OCI).
7. **Rendu du Job** via `operator/templates/package.yaml.hbs` (action `install`).
8. **Création/upsert** du Job (Server-Side Apply, fallback delete+create).
9. Requeue toutes les **15 minutes**.

L'annotation `force-reinstall` supprime le Job existant avant recréation. L'annotation
`suspend=true` court-circuite tout en (1).

## Phases d'installation (côté agent)

Une fois le Job lancé, l'agent dépaquette l'image et exécute le script de cycle de vie
(`agent/scripts/{type}/install.rhai`). Les objets sont appliqués **par phases**, et
l'instance est rechargée entre chaque phase pour propager les mises à jour de statut :

```mermaid
flowchart LR
    PRE[install_pre] --> B[befores]
    B --> V[vitals]
    V --> T[tofu]
    T --> IF[init_from]
    IF --> O[others]
    O --> SC[scalables]
    SC --> P[posts]
    P --> BK{use_backup\n+ secret ?}
    BK -->|oui| SB[schedule_backup]
    BK -->|non| DB[delete_backup]
    SB --> POST[install_post]
    DB --> POST
    POST --> READY[set_status_ready]
```

Voir [Cycle de vie d'un paquet](packages/lifecycle.md) pour le détail des hooks
`*_pre`/`*_post` et la sémantique de chaque phase.

## Désinstallation (finalizer / cleanup)

```mermaid
flowchart TD
    DEL[Suppression de l'instance] --> SEL2[Sélection du paquet]
    SEL2 -->|introuvable + a des enfants| BLOCK[Erreur : finalizer non retiré]
    SEL2 -->|trouvé ou sans enfants| JOB2[Rendu du Job delete]
    JOB2 --> RORD[Suppression dans l'ordre inverse :\nposts → scalables → tofu → others → vitals → befores]
    RORD --> WAIT[Attente de complétion du Job]
    WAIT --> CLEAN[Purge du Job + retrait du finalizer]
```

`do_cleanup<T>()` :

1. Sélection du paquet (même filtre que l'install).
2. Si le paquet est introuvable **et** que l'instance a des enfants
   (`status.have_child()`), une erreur est levée (le finalizer ne se retire pas tant que le
   paquet est introuvable).
3. Sinon : rendu du Job avec action `delete`, exécution du `delete.rhai` qui supprime les
   enfants **dans l'ordre inverse** (posts → scalables → tofu → others → vitals → befores),
   en se basant sur les listes du `status`.
4. Attente de complétion du Job de delete, purge du Job, retrait du finalizer.

> **Limites connues** (voir [Dépannage](operations/troubleshooting.md)) :
> - Si le `type` du paquet a changé depuis l'installation (ex. `tenant` → `service`), la
>   sélection échoue et la désinstallation reste bloquée (issue #12).
> - L'attente de complétion ne détecte pas l'état `Failed` : un Job de delete en échec fait
>   patienter jusqu'au timeout (issue #15).

## Gestion d'erreur et requeue

Chaque contrôleur a une `error_policy` qui logue l'erreur, incrémente les métriques
d'échec et requeue (5 min pour les JukeBox). Les réconciliations réussies requeue à 15 min.
Les opérations bloquantes (attente de suppression/complétion de Job) ont des timeouts
explicites (20 s pour une suppression, 10 min pour un Job de delete).

## Métriques

L'opérateur expose des métriques Prometheus sur `GET /metrics` (port 9000). Quatre
registres (un par type de ressource) exposent : durée des réconciliations (histogramme),
compteurs succès/échec, jauge des réconciliations en cours, horodatage du dernier
événement.
