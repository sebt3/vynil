# Référence des CRD

Toutes les ressources sont dans le groupe **`vynil.solidite.fr/v1`**. Les définitions
faisant foi se trouvent dans [`common/src/`](../common/src/) (types Rust dérivant
`CustomResource`) et sont générées dans [`deploy/crd/crd.yaml`](../deploy/crd/crd.yaml) via
`agent crdgen`.

## JukeBox (cluster-scoped)

Source de paquets. Raccourci : `jb`.

```yaml
apiVersion: vynil.solidite.fr/v1
kind: JukeBox
metadata:
  name: home-alpha
spec:
  source:            # exactement une variante (voir Sources de JukeBox)
    list: ["registry.example.com/org/vynil"]
  maturity: stable   # stable | beta | alpha
  schedule: "0 3 * * *"
  pull_secret: my-pull-secret   # optionnel : Secret de type dockerconfigjson
status:
  packages: []       # cache des paquets scannés (waypoints)
```

| Champ | Type | Description |
|---|---|---|
| `spec.source` | objet | Variante de source : `list`, `harbor`, `gitlab`, `script`, `http`, `s3`. |
| `spec.maturity` | enum | Niveau de maturité retenu lors du scan. |
| `spec.schedule` | cron | Planification du rescan (CronJob). |
| `spec.pull_secret` | string | Secret `dockerconfigjson` pour registre privé. |
| `status.packages` | liste | Catalogue calculé (un waypoint par époque d'upgrade). |

## SystemInstance (namespaced)

Installation d'un paquet **système** (composant cluster). Pas de sauvegarde, pas d'`initFrom`.

```yaml
apiVersion: vynil.solidite.fr/v1
kind: SystemInstance
metadata:
  name: traefik
  namespace: vynil-system
spec:
  jukebox: vynil
  category: networking
  package: traefik
  options: {}
status:
  tag: "3.7.1"
  digest: "<empreinte options>"
  conditions: []
```

## ServiceInstance (namespaced)

Installation d'un paquet **service** (application partagée, CRDs propres, sauvegarde).
Même structure que `TenantInstance` ci-dessous (avec `initFrom`).

## TenantInstance (namespaced)

Installation d'un paquet **tenant**. Raccourci : `vti`.

```yaml
apiVersion: vynil.solidite.fr/v1
kind: TenantInstance
metadata:
  name: gretel
  namespace: epikaf-nan-ia
spec:
  jukebox: home-alpha
  category: think
  package: ollama
  initFrom:                 # optionnel : restauration depuis une sauvegarde
    secretName: backup-settings
    subPath: epikaf-nan-ia/ollama
    snapshot: "abc123"
    version: "0.1.8"        # version de paquet à utiliser pour restaurer
  options:
    use_rocm: true
status:
  tag: "0.1.8-beta.50"
  digest: "<empreinte options>"
  conditions: []
  vitals:    []   # PVC créés
  scalables: []   # Deployment/StatefulSet créés
  others:    []   # Service/ConfigMap/Ingress/…
  befores:   []
  posts:     []
  services:  []   # services publiés (capability registry)
  tfstate:   "…"  # état OpenTofu (gzip+base64), si applicable
  rhaistate: "…"  # état Rhai custom (gzip+base64), si applicable
```

### Spec commune (service/tenant)

| Champ | Type | Description |
|---|---|---|
| `spec.jukebox` | string | Nom de la JukeBox source. |
| `spec.category` | string | Catégorie du paquet. |
| `spec.package` | string | Nom du paquet. |
| `spec.options` | map | Paramètres validés contre le schéma `options` du paquet. |
| `spec.initFrom.secretName` | string | Secret S3/Restic (défaut `backup-settings`). |
| `spec.initFrom.subPath` | string | Préfixe dans le bucket (défaut `<ns>/<app-slug>`). |
| `spec.initFrom.snapshot` | string | Identifiant de snapshot Restic à restaurer. |
| `spec.initFrom.version` | string | Version de paquet exacte pour la restauration. |

### Conditions de statut

Le `status.conditions` reflète l'avancement. Types possibles (tenant) : `Ready`,
`Installed`, `Backuped`, `Restored`, `AgentStarted`, `TofuInstalled`, `BeforeApplied`,
`VitalApplied`, `ScalableApplied`, `InitFrom`, `ScheduleBackup`, `OtherApplied`,
`RhaiApplied`, `PostApplied`. Chaque condition porte un `status` (`True`/`False`), un
`message`, une `generation` et un `lastTransitionTime`.

Exemple de message d'erreur observable : une condition `AgentStarted=False` avec
`message: "Package think/ollama is missing"` indique que l'opérateur n'a pas trouvé le
paquet correspondant dans le cache de la JukeBox.

## Annotations de contrôle

### Sur les instances

| Annotation | Valeur | Effet |
|---|---|---|
| `vynil.solidite.fr/suspend` | `"true"` | Suspend la réconciliation (requeue 15 min, aucune action) jusqu'au retrait. |
| `vynil.solidite.fr/force-reinstall` | présente | Supprime le Job existant et force une réinstallation ; l'annotation est retirée automatiquement. |

### Sur les JukeBox

| Annotation | Valeur | Comportement |
|---|---|---|
| `vynil.solidite.fr/force-scan` | `"true"` (ou présente) | Scan complet. |
| `vynil.solidite.fr/force-scan` | `"<category>"` | Scan partiel d'une catégorie. |
| `vynil.solidite.fr/force-scan` | `"<category>/<name>"` | Scan partiel d'un paquet. |
| `vynil.solidite.fr/last-scan-time` | (géré par l'opérateur) | Horodatage de complétion du dernier scan traité. |

## Finalizers

Chaque ressource pose un finalizer (`<kind>.vynil.solidite.fr`) pour garantir le nettoyage
des objets enfants et des Jobs avant suppression. Voir
[Réconciliation](reconciliation.md) et [Dépannage](operations/troubleshooting.md).
