terraform {
  required_providers {
    kubernetes = {
        source  = "hashicorp/kubernetes"
        version = "~> 2.34.0"
    }
    kubectl = {
        source  = "gavinbunney/kubectl"
        version = "~> 1.16.0"
    }
    postgresql = {
        source  = "cyrilgdn/postgresql"
        version = "~> 1.24.0"
    }
    mysql = {
        source  = "petoju/mysql"
        version = "~> 3.0.67"
    }
  }
}
