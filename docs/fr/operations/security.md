# Sécurité & modèle de menace

Cette page décrit le modèle de sécurité **réel** de Vynil aujourd'hui, ses implications, et
les chantiers d'amélioration identifiés. À lire avant tout déploiement en production.

## Modèle de menace en une phrase

> **Installer un paquet = exécuter du code arbitraire avec les droits cluster-admin.**

Par conséquent : **n'installez que des paquets issus de JukeBox de confiance**, et traitez
le droit de créer une `*Instance` comme équivalent à un accès cluster-admin.

## Pourquoi

### L'agent tourne en cluster-admin

Le ServiceAccount `vynil-agent` est lié à `cluster-admin` **et** à un ClusterRole `*/*/*`
maison ([`box/vynil/systems/rbac.yaml.hbs`](../../../box/vynil/systems/rbac.yaml.hbs)). Le
bootstrap (`vynil-bootstrap`) dispose lui aussi d'un ClusterRole `*/*/*`
([`deploy/bootstrap/bootstrap.yaml`](../../../deploy/bootstrap/bootstrap.yaml)).

### L'agent exécute le code des paquets

L'agent exécute les scripts Rhai embarqués dans l'image OCI du paquet. Le moteur Rhai
« core » expose des primitives puissantes à **tous** les paquets
([`common/src/shellhandler.rs`](../../../common/src/shellhandler.rs),
[`common/src/rhaihandler.rs`](../../../common/src/rhaihandler.rs)) :

- `shell_run` / `shell_output` — exécution shell arbitraire (`sh -c …`) ;
- `get_env` — lecture des variables d'environnement de l'agent ;
- `file_read` / `file_write` / `file_copy` / `create_dir` — accès au système de fichiers.

Combiné au point précédent, un paquet malveillant ou compromis (ou une JukeBox/un registre
compromis) obtient le contrôle total du cluster : lecture de tous les secrets, création de
pods privilégiés, exfiltration, persistance.

### La signature des images n'est pas vérifiée à l'installation

Les images de paquets sont **signées au push** (Cosign — voir
[Build & signature](../build-signing.md)), mais **aucune vérification de signature** n'est
faite au pull/scan/install ([`common/src/ocihandler.rs`](../../../common/src/ocihandler.rs) :
`pull_image`, `verify_tag_in_registry`). La signature n'apporte donc aujourd'hui aucune
garantie de provenance côté consommation.

## Ce qui est correct

- **Génération de mots de passe** : `gen_password` / `gen_password_alphanum` utilisent un
  CSPRNG (`rand`) avec jeux de caractères pondérés.
- **HTTP** : le client HTTP utilise `rustls`.
- **Auth de registre** : les credentials sont lus depuis des Secrets `dockerconfigjson` et
  ne sont pas journalisés.
- **SecurityContext** : les Jobs d'agent tournent en `runAsUser/Group 65534` (nobody),
  `fsGroup 65534` ; la génération de paquets ajoute `runAsNonRoot`, `readOnlyRootFilesystem`
  et `drop ALL` aux conteneurs applicatifs.

## Chantiers d'amélioration (suivis)

| Sujet | Issue |
|---|---|
| Réduire les privilèges de l'agent (moindre privilège par type de paquet, restreindre `shell_run`/`get_env` aux paquets de confiance) | [#13](https://git.kydah.fr/shuss/vynil/issues/13) |
| Vérifier la signature Cosign au pull/install, épingler par digest, clé de confiance par JukeBox | [#14](https://git.kydah.fr/shuss/vynil/issues/14) |

## Recommandations d'exploitation

1. **Restreindre la création d'instances** par RBAC : seuls des opérateurs de confiance
   doivent pouvoir créer des `SystemInstance`/`ServiceInstance`/`TenantInstance`.
2. **Sources de confiance uniquement** : limitez les JukeBox à des registres que vous
   contrôlez ; préférez des registres privés avec `pull_secret`.
3. **Revue des paquets tiers** : auditez les scripts Rhai (et l'usage de `shell_run`) avant
   d'ajouter une catégorie/un paquet à une JukeBox de production.
4. **Défense en profondeur** : en attendant la vérification de signature en cluster,
   utilisez une politique d'admission (Kyverno, Sigstore Policy Controller) avec `cosign.pub`
   pour exiger des images signées.
5. **Isolation réseau** : limitez l'accès sortant de l'agent au strict nécessaire (registre,
   S3 de backup).
