# Registry Maintenance (purging package images)

A Vynil package registry grows continuously: each build publishes a semver tag, plus
associated artifacts (Cosign `.sig`/`.att` signatures, SBOM, scan caches). Periodic
purging is necessary — but it must follow strict **retention rules**, or risk breaking
existing installations.

## The contract: purging must never delete what the scan would expose

The JukeBox scan ([Sources](sources.md)) computes a **reduced view** of the registry: the
version head per maturity level + migration waypoints. Purging is the dual operation: it
may delete everything that will never appear again in this reduced view —
and **nothing else**.

Concretely, a purge must keep:

1. **The head of each maturity channel**: the most recent tag in `alpha`, the most
   recent in `beta`, the most recent `stable`.
2. **Migration waypoints**: any version that a `MinimumPreviousVersion` chain
   requires to allow step-by-step upgrades. Deleting a waypoint prevents old
   installations from being able to update.
3. **The last revision of each package `type`**, if the type has changed during the
   history. An instance installed with the old type needs a revision of that type to
   **uninstall** cleanly (its delete hooks live in the package image — see
   [Lifecycle](../packages/lifecycle.md)). Purging the last `tenant` revision of a
   package that became a `service` makes existing `TenantInstance` instances
   uninstallable (see [Troubleshooting](../operations/troubleshooting.md)).
4. **Artifacts attached to retained tags**: Cosign signatures, attestations, and SBOMs
   referenced by the kept tags. Orphaned artifacts (attached to purged tags) are on
   the contrary good candidates for deletion.

> Rules 1 and 2 can be decided from tag names and *requirements* annotations alone.
> Rule 3 requires reading the **manifest metadata**
> (`fr.solidite.vynil.metadata`) to know the `type` of each revision: a purge that
> only looks at tag strings is blind to type changes.

## Type change = migration

Changing the `type` of a package between two publications is strongly discouraged
([Concepts](../concepts.md)). If unavoidable:

- treat the publication that changes the type as a **migration boundary**: the last
  revision of the old type must remain in the registry as long as instances installed
  with that type may still exist;
- uninstall (or migrate) instances of the old type **before** allowing the purge to
  reclaim the old revision.

## Scan ↔ purge consistency

The scan and the purge apply the **same rules** (semver, maturity, waypoints, types);
implementing them in two separate places creates a risk of drift — a purge more
aggressive than the scan destroys versions the controller still expects. Practical
recommendation: derive the purge script from the same library as the scan (Rhai scripts
embedded in the agent image), and run it via the agent:

```yaml
# Example: purge job scheduled in the distribution CI
schedule: "0 5 * * *"
steps:
- uses: docker://<registry>/vynil-agent:<version>
  with:
    args: run -f .scripts/clean_registry.rhai
```

## Symptoms of overly aggressive purging

| Symptom | Rule violated |
|---|---|
| `Package <cat>/<name> is missing` on an installed instance | 1 or 3 |
| Upgrade refused (`MinimumPreviousVersion` not satisfiable) | 2 |
| Uninstall blocked (finalizer not removed) after type change | 3 |
| `cosign verify` fails on a retained tag | 4 |
