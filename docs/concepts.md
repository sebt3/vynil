# Concepts

Cette page pose le vocabulaire et le modèle mental de Vynil. Tout le reste de la
documentation s'appuie dessus.

## Paquet (package)

Un **paquet Vynil** est une **image OCI** publiée dans un registre. Elle contient :

- des **métadonnées** portées par des annotations OCI (nom, catégorie, type, features,
  prérequis, options, recommandations, `value_script`) ;
- un **contenu** : des templates Handlebars (`*.yaml.hbs`), des manifestes YAML statiques
  et des scripts Rhai décrivant le cycle de vie (`scripts/`).

Côté développement, un paquet est un **répertoire** avec un `package.yaml` (voir
[Format d'un paquet](packages/format.md)) que l'on **packe** (`agent package build`) en
image OCI.

### Type de paquet (`type`)

Le type fixe la portée et les capacités. Il est exposé comme `usage` dans le code.

| Type | Pour quoi | Sauvegarde | CRDs propres |
|---|---|---|---|
| `system` | composant cluster (CNI, ingress, opérateur…) | non | oui (`get_crds/`) |
| `service` | application partagée à l'échelle du cluster | oui | oui |
| `tenant` | application cantonnée au namespace/tenant | oui | non |

Le type détermine aussi quel CRD pilote l'installation (`SystemInstance` /
`ServiceInstance` / `TenantInstance`) et quel jeu de scripts d'agent est utilisé
(`agent/scripts/{system,service,tenant}/`).

> ⚠️ Le `type` d'un paquet peut changer entre deux publications (ex. un paquet
> historiquement `tenant` republié en `service`). C'est un cas réel qui a un impact sur la
> désinstallation — voir [Dépannage](operations/troubleshooting.md).
> C'est cependant très fortement déconseillé : c'est un cas limite qui peut survenir
> pendant la maturation ou le développement d'un paquet. Si un changement de type est
> inévitable, traitez-le comme une migration et assurez-vous que la dernière révision de
> l'ancien type reste disponible dans le registre — voir
> [Maintenance du registre](jukebox/registry-maintenance.md).

### Catégorie (`category`)

Chaîne libre regroupant les paquets (ex. `core`, `networking`, `database`, `think`). Le
couple **`category/name`** identifie un paquet au sein d'une JukeBox.

### Features

Drapeaux déclaratifs : `upgrade`, `backup`, `monitoring`, `high_availability`,
`auto_config`, `auto_scaling`, `deprecated`. Ils décrivent ce que le paquet sait faire.

## JukeBox — la source de paquets

Une `JukeBox` (cluster-scoped) décrit **d'où** viennent les paquets et **quand** les
rescanner. Le scan (un Job d'agent piloté par un CronJob) liste les versions disponibles,
filtre par semver/maturité/compatibilité, calcule les **waypoints d'upgrade**, puis écrit
le catalogue dans `JukeBox.status.packages`. C'est ce cache que l'opérateur consulte pour
chaque installation.

Sources possibles : liste OCI, projet Harbor, projet GitLab, script Rhai, cache HTTP, ou
bucket S3. Voir [Sources de JukeBox](jukebox/sources.md).

### Maturité et waypoints

Une JukeBox a une `maturity` (`stable` | `beta` | `alpha`). Le scan ne conserve pas toutes
les versions : il garde un **waypoint par « époque »** de `MinimumPreviousVersion`, ce qui
permet une mise à jour progressive (chaîne d'upgrade) sans stocker l'historique complet.

Exemple — versions publiées `[4.0(min:3.0), 3.5(min:2.0), 3.0(min:2.0), 2.5, 1.5]`
→ waypoints conservés `[4.0, 3.5, 2.5, 1.5]`.

## Instance — une installation

Une **instance** est une demande d'installation d'un paquet dans un namespace. Les trois
CRD partagent la même mécanique (`spec.jukebox`, `spec.category`, `spec.package`,
`spec.options`) et un `status` qui mémorise la version installée (`status.tag`),
l'empreinte des options (`status.digest`), les conditions et — pour service/tenant — les
**enfants** créés (voir ci-dessous).

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
  options:
    use_rocm: true
    models: [ "qwen3:14b", "mistral-small3.2:24b" ]
```

### Enfants (children) et phases

À l'installation, l'agent applique les objets du paquet en **phases ordonnées**, et
enregistre les objets créés dans le `status` de l'instance par catégorie :

| Phase / champ status | Contenu typique | Ordre install | Ordre delete |
|---|---|---|---|
| `befores` | pré-requis (jobs d'init, secrets) | 1 | dernier |
| `vitals` | données persistantes (PVC) | 2 | avant-dernier |
| *(tofu)* | ressources OpenTofu/Terraform | 3 | — |
| `others` | Service, ConfigMap, Ingress, Role… | 4 | 3ᵉ |
| `scalables` | Deployment, StatefulSet… | 5 | 1ᵉʳ (avec tofu) |
| `posts` | actions finales | 6 | en premier |

L'agent récupère l'instance à jour entre chaque phase. La **désinstallation** procède dans
l'ordre inverse et s'appuie sur ces listes du `status` pour savoir *quoi* supprimer. Le
`status` ne suffit cependant pas à lui seul : les **hooks de delete** embarqués dans
l'image du paquet restent indispensables pour nettoyer les ressources créées
*indirectement* (par exemple les volumes créés par un opérateur tiers, qui ne portent pas
les marqueurs d'appartenance de l'instance). Voir
[Cycle de vie d'un paquet](packages/lifecycle.md).

> La présence d'au moins un enfant (`status.have_child()`) signale qu'une instance a
> réellement déployé quelque chose ; la logique de cleanup en tient compte.

### initFrom — restauration

`spec.initFrom` (service/tenant) permet d'initialiser une nouvelle installation à partir
d'une sauvegarde (snapshot Restic), avec optionnellement une `version` de paquet précise à
utiliser pour la restauration.

## Agent vs Opérateur

- L'**opérateur** (`operator`) surveille les CRD et décide *quoi* faire : il sélectionne le
  paquet dans le cache de la JukeBox, vérifie les prérequis, rend le template du Job, et
  crée/supprime ce Job. Il ne touche jamais directement aux objets applicatifs.
- L'**agent** (`agent`, lancé dans un Job) fait le travail concret : il dépaquette l'image
  OCI, exécute les scripts Rhai du paquet, rend les templates Handlebars et applique les
  objets dans le cluster.

Cette séparation est la clé de l'architecture : voir [Architecture](architecture.md) et
[Réconciliation](reconciliation.md).

## Scripts Rhai et templates Handlebars

- **Handlebars** rend les manifestes Kubernetes à partir du contexte de l'instance
  (helpers `image_from_ctx`, `resources_from_ctx`, `selector_from_ctx`…).
- **Rhai** est le langage de script du cycle de vie. Le moteur expose des fonctions de
  manipulation K8s, OCI, HTTP, S3, secrets, mots de passe, semver, etc. Certaines
  primitives (shell, accès fichiers, variables d'environnement) sont puissantes et doivent
  être considérées dans le [modèle de menace](operations/security.md).
