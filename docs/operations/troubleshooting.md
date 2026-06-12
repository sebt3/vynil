# Dépannage

## Observer l'état

```bash
# état d'une instance et conditions détaillées
kubectl -n <ns> get tenantinstances
kubectl -n <ns> describe tenantinstance <name>

# Jobs d'agent (install/delete/scan) dans le namespace système
kubectl -n vynil-system get jobs
kubectl -n vynil-system logs job/<job-name>

# logs de l'opérateur
kubectl -n vynil-system logs deploy/vynil-controller --since=1h

# catalogue d'une jukebox
kubectl get jukebox <name> -o jsonpath='{.status.packages[*].metadata.name}'
```

## Une instance reste en erreur « Package … is missing »

La condition `AgentStarted=False` avec `message: "Package <cat>/<name> is missing"` signifie
que l'opérateur n'a pas trouvé de paquet correspondant dans le cache de la JukeBox. Causes
possibles :

- la JukeBox n'a pas (re)scanné → forcer un scan :
  `kubectl annotate jukebox <jb> vynil.solidite.fr/force-scan=true --overwrite` ;
- le paquet n'existe pas pour ce `category`/`name`/`type` ;
- le **type** du paquet a changé (voir ci-dessous) ;
- la version minimale d'upgrade (`MinimumPreviousVersion`) exclut la version installée.

## Désinstallation bloquée (finalizer non retiré)

**Symptôme** : `kubectl delete` ne supprime jamais l'instance ; `deletionTimestamp` est posé
mais l'objet persiste. Les logs de l'opérateur bouclent sur :

```text
FinalizerError(CleanupFailed(Other("This install have child but the package cannot be found")))
```

**Cause** : le paquet a été republié avec un **`type` différent** de celui utilisé à
l'installation (cas réel : un paquet `tenant` devenu `service`). La sélection de paquet du
cleanup filtre sur le type ; comme l'instance a des enfants (`status.have_child()` vrai),
l'absence de paquet correspondant lève une erreur dure et le finalizer n'est jamais retiré.
Voir l'analyse complète et le correctif dans
[issue #12](https://git.kydah.fr/shuss/vynil/issues/12).

**Déblocage immédiat** (⚠️ laisse les objets enfants orphelins, à nettoyer manuellement) :

```bash
kubectl -n <ns> patch <kind> <name> --type=json \
  -p '[{"op":"remove","path":"/metadata/finalizers/0"}]'
# puis supprimer à la main les objets listés dans l'ancien status (vitals/scalables/others…)
```

**Correctif de fond** : relâcher la sélection de paquet pour le cleanup (le `status` suffit à
piloter la suppression). Suivi dans l'issue #12.

## Désinstallation lente (~10 min) sur échec

Le cleanup attend la **complétion** du Job de delete sans détecter l'état `Failed` : un Job
de delete qui échoue fait patienter jusqu'au timeout de 10 minutes avant de remonter
l'erreur, à chaque réconciliation. Suivi dans
[issue #15](https://git.kydah.fr/shuss/vynil/issues/15). En attendant, vérifiez les logs du
pod du Job de delete pour la cause réelle de l'échec.

## Un scan ne met pas à jour le catalogue

- vérifiez que le Job `scan-<jukebox>` s'est terminé en `Complete` :
  `kubectl -n vynil-system get jobs | grep scan-` ;
- l'opérateur ne recharge le cache qu'**une fois par complétion** (annotation
  `last-scan-time`) — un Job déjà traité ne re-déclenche pas de rechargement ;
- pour les sources `http`/`s3`, vérifiez que `index.yaml` est à jour côté cache.

## Forcer une réinstallation

```bash
kubectl -n <ns> annotate <kind> <name> vynil.solidite.fr/force-reinstall= --overwrite
```

L'opérateur supprime le Job existant, relance l'installation, puis retire l'annotation.

## Suspendre la réconciliation

```bash
kubectl -n <ns> annotate <kind> <name> vynil.solidite.fr/suspend=true --overwrite
# réactiver
kubectl -n <ns> annotate <kind> <name> vynil.solidite.fr/suspend- 
```
