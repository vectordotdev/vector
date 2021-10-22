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

data "template_file" "soak-observer" {
  template = file("${path.module}/observer.toml.tpl")
  vars = {
    experiment_name = "datadog_agent_remap_datadog_logs"
    vector_id       = var.vector_image
    query           = "sum(rate((bytes_written[1m])))"
  }
}

# Setup background monitoring details. These are needed by the soak control to
# understand what vector et al's running behavior is.
module "monitoring" {
  source        = "../../common/terraform/modules/monitoring"
  observer-toml = data.template_file.soak-observer.rendered
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
  source       = "../../common/terraform/modules/vector"
  type         = var.type
  vector_image = var.vector_image
  sha          = var.sha
  test_name    = "datadog_agent_remap_datadog_logs"
  vector-toml  = file("${path.module}/vector.toml")
  namespace    = kubernetes_namespace.soak.metadata[0].name
  depends_on   = [module.http-blackhole]
}
module "http-blackhole" {
  source              = "../../common/terraform/modules/lading_http_blackhole"
  type                = var.type
  http-blackhole-toml = file("${path.module}/http_blackhole.toml")
  namespace           = kubernetes_namespace.soak.metadata[0].name
}
module "http-gen" {
  source        = "../../common/terraform/modules/lading_http_gen"
  type          = var.type
  http-gen-toml = file("${path.module}/http_gen.toml")
  namespace     = kubernetes_namespace.soak.metadata[0].name
}
