# Maintenance du registre (purge des images de paquets)

Un registre de paquets Vynil grossit en continu : chaque build publie un tag semver, plus
des artefacts associés (signatures Cosign `.sig`/`.att`, SBOM, caches de scan). Une purge
périodique est nécessaire — mais elle doit respecter des **règles de rétention** strictes,
sous peine de casser des installations existantes.

## Le contrat : la purge ne doit jamais supprimer ce que le scan exposerait

Le scan de JukeBox ([Sources](sources.md)) calcule une **vue réduite** du registre : tête
de version par niveau de maturité + waypoints de migration. La purge est l'opération
duale : elle peut supprimer tout ce qui n'apparaîtra plus jamais dans cette vue réduite —
et **rien d'autre**.

Concrètement, une purge doit conserver :

1. **La tête de chaque canal de maturité** : le tag le plus récent en `alpha`, le plus
   récent en `beta`, le plus récent `stable`.
2. **Les waypoints de migration** : toute version qu'une chaîne de
   `MinimumPreviousVersion` rend nécessaire pour permettre les mises à jour par étapes.
   Supprimer un waypoint condamne les installations anciennes à ne plus pouvoir se mettre
   à jour.
3. **La dernière révision de chaque `type` de paquet**, si le type a changé au cours de
   l'historique. Une instance installée avec l'ancien type a besoin d'une révision de ce
   type pour se **désinstaller** proprement (ses hooks de delete vivent dans l'image du
   paquet — voir [Cycle de vie](../packages/lifecycle.md)). Purger la dernière révision
   `tenant` d'un paquet devenu `service` rend les instances `TenantInstance` existantes
   indésinstallables (voir [Dépannage](../operations/troubleshooting.md)).
4. **Les artefacts attachés aux tags conservés** : signatures Cosign, attestations et SBOM
   référencés par les tags gardés. Les artefacts orphelins (rattachés à des tags purgés)
   sont au contraire de bons candidats à la suppression.

> Les règles 1 et 2 se décident à partir des seuls noms de tags et des annotations de
> *requirements*. La règle 3 impose de lire les **métadonnées** des manifests
> (`fr.solidite.vynil.metadata`) pour connaître le `type` de chaque révision : une purge
> qui ne regarde que les chaînes de tags est aveugle aux changements de type.

## Changement de type = migration

Changer le `type` d'un paquet entre deux publications est très fortement déconseillé
([Concepts](../concepts.md)). Si c'est inévitable :

- considérez la publication qui change le type comme une **frontière de migration** : la
  dernière révision de l'ancien type doit rester dans le registre tant qu'il peut exister
  des instances installées avec ce type ;
- désinstallez (ou migrez) les instances de l'ancien type **avant** de laisser la purge
  réclamer l'ancienne révision.

## Cohérence scan ↔ purge

Le scan et la purge appliquent les **mêmes règles** (semver, maturité, waypoints, types) ;
les implémenter à deux endroits différents crée un risque de dérive — une purge plus
agressive que le scan détruit des versions que le contrôleur attend encore. Recommandation
pratique : dériver le script de purge de la même bibliothèque que le scan (scripts Rhai
embarqués dans l'image de l'agent), et l'exécuter via l'agent :

```yaml
# Exemple : job de purge planifié dans la CI de la distribution
schedule: "0 5 * * *"
steps:
- uses: docker://<registry>/vynil-agent:<version>
  with:
    args: run -f .scripts/clean_registry.rhai
```

## Symptômes d'une purge trop agressive

| Symptôme | Règle violée |
|---|---|
| `Package <cat>/<name> is missing` sur une instance installée | 1 ou 3 |
| Upgrade refusé (`MinimumPreviousVersion` non satisfiable) | 2 |
| Désinstallation bloquée (finalizer non retiré) après changement de type | 3 |
| `cosign verify` échoue sur un tag conservé | 4 |
