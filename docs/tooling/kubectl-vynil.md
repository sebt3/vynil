# kubectl-vynil — CLI plugin for kubectl

`kubectl-vynil` is a `kubectl` plugin that drives the Vynil operator from the user's kubeconfig context.
It is invoked as `kubectl vynil …` or directly as `kubectl-vynil …`.

## Installation

`kubectl-vynil` and its `kubectl_complete-vynil` completion shim ship in the same CLI release
archive. Put **both** on your `PATH` (e.g. `install -m 0755 kubectl-vynil kubectl_complete-vynil
~/.local/bin/`). The shim is what `kubectl` invokes for `kubectl vynil <TAB>` completion.

## Command syntax

```
kubectl-vynil [--context <ctx>] [-n <namespace>] <kind> <name> <verb> [args]
```

| Segment | Description |
|---|---|
| `--context <ctx>` | Kubernetes context (optional; uses kubeconfig default if omitted). |
| `-n <namespace>` | Kubernetes namespace for the instance (required for namespaced resources; ignored for `JukeBox`). |
| `<kind>` | Resource type: `jukebox` (or `box`), `vti`, `vsvc`, `vsi`. Aliases: `tenantinstance` (→ `vti`), `serviceinstance` (→ `vsvc`), `systeminstance` (→ `vsi`). |
| `<name>` | Name of the resource instance in the cluster. |
| `<verb>` | Action to perform on the resource (see table below). |

## Available verbs by kind

| Kind | Verbs |
|---|---|
| `jukebox` (box) | `scan` — trigger a package scan of the JukeBox source. |
| `vti` (TenantInstance) | `upgrade`, `scan`, `diagnostic`, `children`, `agentlog`, `childlogs`, `operatorlog`. |
| `vsvc` (ServiceInstance) | same as `vti`. |
| `vsi` (SystemInstance) | same as `vti`. |

## Dynamic shell completion

`kubectl-vynil` supports dynamic shell completion for kinds, resource names, verbs, and flags.
Completion works in two modes:

1. **Via kubectl plugin** (`kubectl vynil <TAB>`): requires `kubectl_complete-vynil` on your `PATH`.
2. **Direct invocation** (`kubectl-vynil <TAB>`): source the completion script in your shell:
   ```sh
   source <(kubectl-vynil completion bash)  # or: zsh
   ```

Both modes reach the same engine: the hidden `kubectl-vynil __complete` verb, which emits Cobra-format
completions and queries the cluster for live resource names.
