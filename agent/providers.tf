terraform {
  required_providers {
    kustomization = {
        source  = "kbst/kustomization"
        version = "~> 0.9.2"
    }
    kubernetes = {
        source = "hashicorp/kubernetes"
        version = "~> 2.20.0"
    }
    kubectl = {
        source = "gavinbunney/kubectl"
        version = "~> 1.14.0"
    }
    authentik = {
        source = "goauthentik/authentik"
        version = "~> 2023.5.0"
    }
    postgresql = {
        source = "cyrilgdn/postgresql"
        version = "~> 1.19.0"
    }
      http = {
        source = "hashicorp/http"
        version = "~> 3.3.0"
    }
      restapi = {
        source = "Mastercard/restapi"
        version = "~> 1.18.0"
      }
  }
}
