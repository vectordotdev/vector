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
  source = "../../..//terraform/modules/monitoring"
  type         = var.type
  vector_image = var.vector_image
}

# Setup the soak pieces
#
# This soak config sets up a vector soak with lading/tcp-gen feeding into vector,
# lading/http-blackhole receiving.
resource "kubernetes_namespace" "soak" {
  metadata {
    name = "soak"
  }
}

module "vector" {
  source       = "../../..//terraform/modules/vector"
  type         = var.type
  vector_image = var.vector_image
  test_name    = "syslog_loki"
  vector-toml  = file("${path.module}/vector.toml")
  namespace    = kubernetes_namespace.soak.metadata[0].name
  depends_on   = [module.http-blackhole]
}
module "http-blackhole" {
  source              = "../../..//terraform/modules/lading_http_blackhole"
  type                = var.type
  http-blackhole-toml = file("${path.module}/http_blackhole.toml")
  namespace           = kubernetes_namespace.soak.metadata[0].name
}
module "tcp-gen" {
  source       = "../../..//terraform/modules/lading_tcp_gen"
  type         = var.type
  tcp-gen-toml = file("${path.module}/tcp_gen.toml")
  namespace    = kubernetes_namespace.soak.metadata[0].name
  depends_on   = [module.vector]
}
