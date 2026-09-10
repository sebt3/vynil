# kubectl-vynil — plugin CLI pour kubectl

`kubectl-vynil` est un plugin `kubectl` qui pilote l'opérateur Vynil depuis le contexte kubeconfig
de l'utilisateur. Il s'invoque comme `kubectl vynil …` ou directement comme `kubectl-vynil …`.

## Installation

`kubectl-vynil` et son shim de complétion `kubectl_complete-vynil` sont livrés dans la même
archive de release CLI. Placer les **deux** dans le `PATH` (ex. `install -m 0755 kubectl-vynil
kubectl_complete-vynil ~/.local/bin/`). Le shim est ce que `kubectl` invoque pour la complétion
`kubectl vynil <TAB>`.

## Syntaxe

```
kubectl-vynil [--context <ctx>] [-n <namespace>] <kind> <name> <verb> [args]
```

| Segment | Description |
|---|---|
| `--context <ctx>` | Contexte Kubernetes (optionnel ; contexte courant du kubeconfig si omis). |
| `-n <namespace>` | Namespace de l'instance (requis pour les ressources namespacées ; ignoré pour `JukeBox`). |
| `<kind>` | Type de ressource : `jukebox` (ou `box`), `vti`, `vsvc`, `vsi`. Alias : `tenantinstance` (→ `vti`), `serviceinstance` (→ `vsvc`), `systeminstance` (→ `vsi`). |
| `<name>` | Nom de l'instance de ressource dans le cluster. |
| `<verb>` | Action à effectuer sur la ressource (voir table ci-dessous). |

## Verbes disponibles par kind

| Kind | Verbes |
|---|---|
| `jukebox` (box) | `scan` — déclenche un scan des paquets de la source du JukeBox. |
| `vti` (TenantInstance) | `upgrade`, `scan`, `diagnostic`, `children`, `agentlog`, `childlogs`, `operatorlog`. |
| `vsvc` (ServiceInstance) | identique à `vti`. |
| `vsi` (SystemInstance) | identique à `vti`. |

## Complétion shell dynamique

`kubectl-vynil` fournit une complétion shell dynamique pour les kinds, les noms de ressources,
les verbes et les drapeaux. Elle fonctionne dans deux modes :

1. **Via le plugin kubectl** (`kubectl vynil <TAB>`) : nécessite `kubectl_complete-vynil` dans le `PATH`.
2. **Appel direct** (`kubectl-vynil <TAB>`) : sourcer le script de complétion dans le shell :
   ```sh
   source <(kubectl-vynil completion bash)  # ou : zsh
   ```

Les deux modes passent par le même moteur : le verbe caché `kubectl-vynil __complete`, qui émet des
complétions au format Cobra et interroge le cluster pour les noms de ressources existants.
