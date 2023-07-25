# Vynil

Vynil is a terraform based installer for kubernetes.
The goal is to build a package manager akin to dpkg/rpm but for the kubernetes.

Unlike helm, kustomize, argoCD, Flux... which all give you all the flexibility to install as you please. Vynil main goal is to help create an integrated distribution for kubernetes, so customisation come scarse but integration of everything by default.
Vynil differ also from openshift since olm can only install operators. Requiering an operator to manage an app while there is already a pseudo-generic installtion operator is madness. Olm should be able to install awx and phpmyadmin, but instead, you need a tower operator to install awx (as if the main use case is running many instances AWX instances). You even need an operator to install kube-virt while there can only be a single instance of kubevirt installed on a k8s cluster. Yet again, this design is madness... Redhat used to known how to install things properly /rant off

## Installation
```
kubectl create ns vynil
kubectl apply -k github.com/sebt3/vynil//deploy
```
## Roadmap

### 0.3.0
Goal : Controlling upgrades
- Agent: At plan stage, detect if some object have to be created/modified, mark the install as "to be upgraded" if so
- Operator: React to annotation "vynil.../apply by starting an "apply" agent and remove the annotation

### 0.4.0
Goal : Upgrade failure management
- Agent: support auto-importing missing ressources
- Operator: React to annotation "vynil.../import by stating an "import" agent and remove the annotation
- Operator: React to annotation "vynil.../pod by stating a pod having plan done running "cat" in the agent image and remove the annotation

### 0.5.0
Goal : Better CRD management
- add a requiered_crds fields on Component
- Agent : Check if all required CRDs existing
    * At the end of the plan stage
    * check if all the custom objects either fit an existing CRDs or part of the current package
- Operator: Validating that requiered CRDs are there before starting an install

### 0.6.0
Goal: Validation webhook
- for Dists
    * Valid URL
    * if secret provided, that they exist
- for Installs
    * Existing dist
    * Existing Componant
    * Existing dependencies (installs and crds ; doesnt requiere "installed" at validation)


## usefull commands

- dist update
```
kubectl config set-context --current --namespace=${VYNIL_NS:=vynil};
kubectl delete job ${VYNIL_DIST:=core}-upg;kubectl create job ${VYNIL_DIST}-upg --from=cronjob/${VYNIL_DIST}-clone
```

- get a component commit_id
```
kubectl get dist ${VYNIL_DIST:=core} -o "jsonpath={.status.components.${VYNIL_CAT:-core}.${VYNIL_COMP:-k8up}.commit_id}{\"\n\"}"
```

- Reset an install status
```
kubectl patch install -n <ns> <inst> --subresource=status -p '{"status":{"status":"","errors":[], "commit_id": ""}}' --type=merge
```

## Private distribution authentication

### https
```
kubectl create secret generic git-creds --from-literal='creds=https://user:token@git.server'
kubectl apply -f - <<ENDyaml
apiVersion: vynil.solidite.fr/v1
kind: Distrib
metadata:
    name: test-https
spec:
    url: https://git.server/vynil/test.git
    insecure: true
    login:
        git_credentials:
            name: git-creds
            key: creds
    schedule: 0 4 * * *
ENDyaml
```

### ssh
```
kubectl create secret generic git-keys --from-file=privatekey=$HOME/.ssh/id_rsa
kubectl apply -f - <<ENDyaml
apiVersion: vynil.solidite.fr/v1
kind: Distrib
metadata:
    name: test-ssh
spec:
    url: ssh://git@git.server/vynil/test.git
    login:
        ssh_key:
            name: git-keys
            key: privatekey
    schedule: 0 5 * * *
ENDyaml
```

