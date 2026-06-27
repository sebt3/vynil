# Troubleshooting

## Observing state

```bash
# instance state and detailed conditions
kubectl -n <ns> get tenantinstances
kubectl -n <ns> describe tenantinstance <name>

# agent Jobs (install/delete/scan) in the system namespace
kubectl -n vynil-system get jobs
kubectl -n vynil-system logs job/<job-name>

# operator logs
kubectl -n vynil-system logs deploy/vynil-controller --since=1h

# jukebox catalog
kubectl get jukebox <name> -o jsonpath='{.status.packages[*].metadata.name}'
```

## An instance stays in error «Package … is missing»

The `AgentStarted=False` condition with `message: "Package <cat>/<name> is missing"` means
the operator did not find a matching package in the JukeBox cache. Possible causes:

- the JukeBox has not (re)scanned → force a scan:
  `kubectl annotate jukebox <jb> vynil.solidite.fr/force-scan=true --overwrite`;
- the package does not exist for this `category`/`name`/`type`;
- the package **type** has changed (see below);
- the minimum upgrade version (`MinimumPreviousVersion`) excludes the installed version.

## Blocked uninstallation (finalizer not removed)

**Symptom**: `kubectl delete` never removes the instance; `deletionTimestamp` is set but
the object persists. The operator logs loop on:

```text
FinalizerError(CleanupFailed(Other("This install have child but the package cannot be found")))
```

**Immediate cause**: no package revision with the **`type` expected** by the instance is
available in the catalog (real case: a `tenant` package republished as `service`). Because
the instance has children (`status.have_child()` true), the missing package raises a hard
error and the finalizer is never removed.

**Root causes** — this is generally the combination of two problems on the registry/scan
side that creates this situation:

- an **overly aggressive purge** of the registry deleted the last revision of the old type
  (see [Registry maintenance](../jukebox/registry-maintenance.md));
- the **scan** only descends the tag history down to the first waypoint and therefore does
  not expose an old revision of a different type, even if it still exists.

**Why no automatic «status-only» delete**: the `status` lists allow deleting what the agent
created directly, but not what the package created *indirectly* (third-party operator
volumes…) — that cleanup lives in the `delete_*` hooks of the package image. A delete
without an image leaves residues; it can only be an action **explicitly requested** by the
human operator, not an automatic fallback. See the full analysis in
[issue #12](https://git.kydah.fr/shuss/vynil/issues/12).

**Immediate unblocking** (⚠️ leaves child objects orphaned, to be cleaned up manually):

```bash
kubectl -n <ns> patch <kind> <name> --type=json \
  -p '[{"op":"remove","path":"/metadata/finalizers/0"}]'
# then manually delete the objects listed in the old status (vitals/scalables/others…)
```

**Long-term fixes** (tracked in issue #12): purge and scan made aware of the package
`type`, plus an opt-in annotation to allow a degraded delete without image when the package
has genuinely disappeared.

## Slow uninstallation (~10 min) on failure

The cleanup waits for **completion** of the delete Job without detecting the `Failed` state:
a failing delete Job waits up to the 10-minute timeout before surfacing the error, at each
reconciliation. Tracked in [issue #15](https://git.kydah.fr/shuss/vynil/issues/15). In the
meantime, check the logs of the delete Job's pod for the actual cause of failure.

## A scan doesn't update the catalog

- check that the `scan-<jukebox>` Job completed as `Complete`:
  `kubectl -n vynil-system get jobs | grep scan-`;
- the operator only reloads the cache **once per completion** (`last-scan-time` annotation)
  — a Job already processed does not re-trigger a reload;
- for `http`/`s3` sources, verify that `index.yaml` is up to date on the cache side.

## Forcing a reinstall

```bash
kubectl -n <ns> annotate <kind> <name> vynil.solidite.fr/force-reinstall= --overwrite
```

The operator deletes the existing Job, relaunches the installation, then removes the
annotation.

## Suspending reconciliation

```bash
kubectl -n <ns> annotate <kind> <name> vynil.solidite.fr/suspend=true --overwrite
# re-enable
kubectl -n <ns> annotate <kind> <name> vynil.solidite.fr/suspend- 
```
