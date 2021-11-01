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

module "monitoring" {
  source = "../../..//terraform/modules/monitoring"
  type         = var.type
  vector_image = var.vector_image
}

resource "kubernetes_namespace" "soak" {
  metadata {
    name = "soak"
  }
}

module "vector" {
  source       = "../../..//terraform/modules/vector"
  type         = var.type
  vector_image = var.vector_image
  test_name    = "syslog_splunk_hec_logs"
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
  source        = "../../..//terraform/modules/lading_tcp_gen"
  type          = var.type
  tcp-gen-toml = file("${path.module}/tcp_gen.toml")
  namespace     = kubernetes_namespace.soak.metadata[0].name
  depends_on   = [module.vector]
}
