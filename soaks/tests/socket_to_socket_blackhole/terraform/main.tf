terraform {
  required_providers {
    kubernetes = {
      version = "~> 2.5.0"
      source  = "hashicorp/kubernetes"
    }
  }
}

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
  depends_on      = [module.vector, module.tcp-blackhole, module.tcp-gen]
}

# Setup the soak pieces
#
# This soak config sets up a vector soak with lading/tcp-gen feeding into vector,
# lading/tcp-blackhole receiving.
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
  depends_on   = [module.tcp-blackhole]
}
module "tcp-blackhole" {
  source              = "../../../common/terraform/modules/lading_tcp_blackhole"
  type                = var.type
  tcp-blackhole-yaml = file("${path.module}/../../../common/configs/tcp_blackhole.yaml")
  namespace           = kubernetes_namespace.soak.metadata[0].name
  lading_image        = var.lading_image
}
module "tcp-gen" {
  source       = "../../../common/terraform/modules/lading_tcp_gen"
  type         = var.type
  tcp-gen-yaml = file("${path.module}/../../../common/configs/tcp_gen_syslog_source.yaml")
  namespace    = kubernetes_namespace.soak.metadata[0].name
  lading_image = var.lading_image
  depends_on   = [module.vector]
}
