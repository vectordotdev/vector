# Set up the providers needed to run this soak and other terraform related
# business. Here we only require 'kubernetes' to interact with the soak
# minikube.
terraform {
  required_providers {
    kubernetes = {
      version = "~> 2.5.0"
      source  = "hashicorp/kubernetes"
    }
  }
}

# Rig the kubernetes provider to communicate with minikube. The details of
# adjusting `~/.kube/config` are addressed by the soak control scripts.
provider "kubernetes" {
  config_path = "~/.kube/config"
}

# Setup background monitoring details. These are needed by the soak control to
# understand what vector et al's running behavior is.
module "monitoring" {
  source        = "../../../common/terraform/modules/monitoring"
  type         = var.type
  vector_image = var.vector_image
}

# Setup the soak pieces
#
# This soak config sets up a vector soak with lading/http-gen feeding into vector,
# lading/http-blackhole receiving.
resource "kubernetes_namespace" "soak" {
  metadata {
    name = "soak"
  }
}

module "vector" {
  source       = "../../../common/terraform/modules/vector"
  type         = var.type
  vector_image = var.vector_image
  test_name    = "datadog_agent_remap_blackhole"
  vector-toml  = file("${path.module}/vector.toml")
  namespace    = kubernetes_namespace.soak.metadata[0].name
}
module "http-gen" {
  source        = "../../../common/terraform/modules/lading_http_gen"
  type          = var.type
  http-gen-toml = file("${path.module}/http_gen.toml")
  namespace     = kubernetes_namespace.soak.metadata[0].name
}
