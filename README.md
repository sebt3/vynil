
# Vynil
Vynil is an installer for kubernetes intended to be used either at home or for SaaS. The goal is to build a package manager akin to dpkg/rpm but for the kubernetes.

Unlike helm, kustomize, argoCD, Flux... which all give you all the flexibility to install as you please. Vynil main goal is to help create an integrated distribution for kubernetes, so customisation come scarse but integration of everything by default. Vynil differ also from openshift since olm can only install operators. Requiering an operator to manage an app while there is already a pseudo-generic installtion operator is madness. Olm should be able to install awx and phpmyadmin, but instead, you need a tower operator to install awx (as if the main use case is running many instances AWX instances). You even need an operator to install kube-virt while there can only be a single instance of kubevirt installed on a k8s cluster. Yet again, this design is madness... Redhat used to known how to install things properly /rant off

## Installation
```
kubectl create ns vynil-system
kubectl apply -k github.com/sebt3/vynil//deploy
```
