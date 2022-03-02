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
  source          = "../../../common/terraform/modules/monitoring"
  experiment_name = var.experiment_name
  variant         = var.type
  vector_image    = var.vector_image
  depends_on      = [module.vector, module.splunk-hec-gen]
}

# Setup the soak pieces
#
# This soak config sets up a vector soak with lading/splunk-hec-gen feeding into vector,
# blackhole receiving.
resource "kubernetes_namespace" "soak" {
  metadata {
    name = "soak"
  }
}

module "vector" {
  source       = "../../../common/terraform/modules/vector"
  type         = var.type
  vector_image = var.vector_image
  vector-toml  = file("${path.module}/vector.toml")
  namespace    = kubernetes_namespace.soak.metadata[0].name
  vector_cpus  = var.vector_cpus
}
module "splunk-hec-gen" {
  source              = "../../../common/terraform/modules/lading_splunk_hec_gen"
  type                = var.type
  splunk-hec-gen-yaml = file("${path.module}/../../../common/configs/splunk_hec_gen.yaml")
  namespace           = kubernetes_namespace.soak.metadata[0].name
  lading_image        = var.lading_image
  depends_on          = [module.vector]
}
