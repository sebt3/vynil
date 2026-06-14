# Référence

## Variables d'environnement de l'opérateur

| Variable | Défaut | Rôle |
|---|---|---|
| `CONTROLLER_BASE_DIR` | `./operator` | Répertoire des templates Handlebars. |
| `VYNIL_NAMESPACE` | `vynil-system` | Namespace système de Vynil. |
| `AGENT_IMAGE` | image compilée par défaut | Image de l'agent utilisée pour les Jobs. |
| `AGENT_ACCOUNT` | `vynil-agent` | ServiceAccount des Jobs d'agent. |
| `AGENT_LOG_LEVEL` | `info` | Niveau de log des Jobs d'agent. |
| `TENANT_LABEL` | `vynil.solidite.fr/tenant` | Clé de label identifiant un tenant. |
| `SCAN_PACKAGE` | (absent) | Filtre partiel pour `box scan` / `box file-scan`. |

> `AGENT_IMAGE` doit suivre la version de l'opérateur. Vérifiez la valeur réelle déployée
> plutôt qu'une valeur codée en dur dans la documentation.

## Variables d'environnement de l'agent

Voir les flags équivalents dans la [Référence CLI](../cli.md) : `NAMESPACE`, `INSTANCE`,
`VYNIL_NAMESPACE`, `PACKAGE_DIRECTORY`, `SCRIPT_DIRECTORY`, `TEMPLATE_DIRECTORY`,
`CONFIG_DIR`, `CONTROLLER_VALUES`, `AGENT_IMAGE`, `TAG`, `LOG_LEVEL`, `SIGNING_KEY`,
`JUNIT_OUTPUT_FILENAME`, `TEMPLATE_OUTPUT_FILENAME`, `TESTSETS_DIRECTORY`, `TEST_NAME`.

## Métriques Prometheus

Exposées sur `GET /metrics` (port 9000, format OpenMetrics). Quatre registres (JukeBox,
System, Service, Tenant) exposent par type :

- durée des réconciliations (histogramme) ;
- compteurs de succès/échec ;
- jauge des réconciliations en cours ;
- horodatage du dernier événement.

## Templates Handlebars de l'opérateur

Répertoire [`operator/templates/`](../../../operator/templates/) :

| Template | Usage |
|---|---|
| `package.yaml.hbs` | Job d'installation/suppression d'une instance. |
| `cronscan.yaml.hbs` | CronJob de scan d'une JukeBox. |
| `scan.yaml.hbs` | Job de scan manuel d'une JukeBox. |

Variables systématiquement présentes dans le contexte : `tag`, `image`, `registry`,
`namespace`, `name`, `job_name`, `package_type`, `package_action`, `digest`, `ctrl_values`,
`rec_crds`, `rec_system_services`, `rec_tenant_services`.

## Helpers Handlebars des paquets

| Helper | Signature | Description |
|---|---|---|
| `image_from_ctx` | `(ctx "key")` | `registry/repository:tag` depuis `package.yaml[images][key]`. |
| `resources_from_ctx` | `(ctx "key")` | `{requests, limits}` depuis `package.yaml[resources][key]`. |
| `selector_from_ctx` | `(ctx comp="key")` | Labels de selector pour le composant. |
| `labels_from_ctx` | `(ctx)` | Labels complets du pod template. |
| `json_to_str` | `(value)` | Sérialise un objet en JSON inline. |
| `ctx_have_crd` | `(ctx "group/version/kind")` | Vrai si le CRD est installé. |

Voir [Génération de paquets](../gen-package.md) pour l'usage complet.

## Stratégie YAML

| Usage | Bibliothèque | Ordre des clés |
|---|---|---|
| Code Rust (serde) | `serde_yaml` | alphabétique |
| `yaml_*_ordered` (Rhai) | `rust-yaml` | préservé |

`rust-yaml` est utilisé pour `package.yaml` (préservation de l'ordre, block scalars
intacts) ; `serde_yaml` partout ailleurs. Le type `YamlError(String)` encapsule les deux.

## Structure du dépôt

```text
vynil/
├── common/      bibliothèque partagée (CRDs, moteurs Rhai/Handlebars, handlers)
├── operator/    contrôleur Kubernetes (+ templates)
├── agent/       CLI exécuté dans les Jobs (+ scripts Rhai)
├── box/         paquets sources (vynil, test)
├── deploy/      kustomize : CRDs + bootstrap
└── docs/        cette documentation
```

Voir [Architecture](../architecture.md) pour le détail des crates.
