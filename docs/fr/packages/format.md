# Format d'un paquet

Un paquet existe sous deux formes : un **répertoire source** (pendant le développement) et
une **image OCI** (une fois packé). Cette page décrit les deux.

## Répertoire source

```text
my-package/
├── package.yaml          # manifeste (métadonnées, images, resources, options, requirements)
├── befores/              # phase 1 — templates .yaml.hbs / .yaml
├── vitals/               # phase 2 — données persistantes (PVC)
├── tofu/                 # phase 3 — fichiers OpenTofu/Terraform (*.tf)
├── others/               # phase 4 — Service, ConfigMap, Ingress, Role…
├── scalables/            # phase 5 — Deployment, StatefulSet…
├── posts/                # phase 6 — actions finales
├── crds/                 # CRDs (system/service uniquement)
├── handlebars/           # partials Handlebars réutilisables
└── scripts/              # hooks Rhai du cycle de vie (*.rhai)
```

Les répertoires de phase sont **optionnels** : l'agent ne traite que ceux présents (voir
[Cycle de vie](lifecycle.md)). Les fichiers `*.yaml.hbs` sont rendus avec le contexte de
l'instance ; les `*.yaml` sont appliqués tels quels.

> Note de nomenclature : le générateur (`gen_package.rhai`) écrit dans des répertoires
> préfixés `get_` (`get_vitals/`, `get_scalables/`, `get_others/`, `get_systems/`,
> `get_crds/`) qui correspondent aux phases ci-dessus côté agent.

## `package.yaml`

```yaml
---
apiVersion: vinyl.solidite.fr/v1beta1
kind: Package
metadata:
  name: traefik             # identifiant (appslug dans les templates)
  category: networking      # catégorie libre
  type: system              # system | service | tenant
  app_version: "3.7.1"      # version de l'application
  description: Traefik ingress controller.
  features:
    - upgrade
    - auto_config
  # backup_affinity: controller   # composant servant d'affinité requise aux jobs de backup
images:
  traefik:                  # clé arbitraire, référencée par {{image_from_ctx this "traefik"}}
    registry: ghcr.io
    repository: traefik/traefik
    tag: v3.7.1             # mis à jour par `agent package update`
resources:                  # requests/limits par conteneur
  traefik:
    requests: { cpu: 100m, memory: 128Mi }
    limits:   { cpu: 1000m, memory: 256Mi }
requirements: []            # dépendances et prérequis
recommandations: []         # dépendances optionnelles déclenchant une mise à jour au changement (ex. monitoring)
options:                    # schéma des paramètres configurables
  replicas:
    type: integer
    default: 1
    description: Nombre de réplicas
```

### Métadonnées (`metadata`)

| Champ | Obligatoire | Description |
|---|---|---|
| `name` | oui | Identifiant du paquet (devient `instance.appslug`). |
| `category` | oui | Catégorie de regroupement. |
| `type` | oui | `system`, `service` ou `tenant`. |
| `app_version` | recommandé | Version de l'application embarquée. |
| `description` | oui | Description lisible. |
| `features` | non | `upgrade`, `backup`, `monitoring`, `high_availability`, `auto_config`, `auto_scaling`, `deprecated`. |
| `backup_affinity` | non | Composant utilisé comme affinité de pod requise pour les jobs de sauvegarde. |

### Prérequis (`requirements`)

Liste de contraintes vérifiées par l'opérateur avant d'installer
(`VynilPackageRequirement`) :

| Variante | Vérifie |
|---|---|
| `MinimumPreviousVersion` | version minimale déjà installée pour autoriser l'upgrade |
| `VynilVersion` | version minimale du framework Vynil |
| `ClusterVersion` | version minimale de Kubernetes |
| `CustomResourceDefinition` | présence d'un CRD donné |
| `SystemService` / `TenantService` | présence d'un service fourni par un autre paquet |
| `Cpu` / `Memory` / `Disk` | ressources disponibles |
| `StorageCapability` | capacité de stockage (`RWX` / `ROX`) |
| `Prefly` | script Rhai de vérification personnalisée |

### Options (`options`)

Schéma (style OpenAPI) des paramètres acceptés dans `spec.options` des instances. Les
options sont validées (`validate_options`) puis exposées au contexte de rendu. Toute valeur
fournie dans `spec.options` est de l'entrée utilisateur : le rendu doit en tenir compte.

### Recommandations & `value_script`

- `recommandations` : listes optionnelles (CRDs, services système/tenant) dont la présence
  active des fonctionnalités supplémentaires sans être bloquante.
- `value_script` : script Rhai évalué par l'opérateur pour produire des valeurs de contrôle
  (`ctrl_values`) injectées dans le contexte Handlebars.

## Image OCI (paquet packé)

`agent package build` (ou `package unpack` pour l'inverse) transforme le répertoire en
image OCI. Les métadonnées sont portées par des **annotations OCI** :

| Annotation | Contenu (JSON, sauf indication) |
|---|---|
| `fr.solidite.vynil.metadata` | nom, catégorie, type, app_version, features |
| `fr.solidite.vynil.requirements` | liste des prérequis |
| `fr.solidite.vynil.options` | schéma des options |
| `fr.solidite.vynil.recommandations` | recommandations |
| `fr.solidite.vynil.value_script` | script Rhai (chaîne) |

Le contenu de l'image (la couche) embarque les répertoires de phase, les scripts Rhai et
les templates Handlebars. L'agent monte ce contenu (`unpack`) avant d'exécuter le cycle de
vie.

Voir [Génération de paquets](../gen-package.md) pour produire ces répertoires depuis un
chart Helm ou des manifestes bruts, et [Build & signature](../build-signing.md) pour la
publication signée.
