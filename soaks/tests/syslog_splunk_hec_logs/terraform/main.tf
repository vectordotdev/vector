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
  source = "../../../common/terraform/modules/monitoring"
  type         = var.type
  vector_image = var.vector_image
}

resource "kubernetes_namespace" "soak" {
  metadata {
    name = "soak"
  }
}

module "vector" {
  source       = "../../../common/terraform/modules/vector"
  type         = var.type
  vector_image = var.vector_image
  test_name    = "syslog_splunk_hec_logs"
  vector-toml  = file("${path.module}/vector.toml")
  namespace    = kubernetes_namespace.soak.metadata[0].name
  depends_on   = [module.monitoring, module.http-blackhole]
}
module "http-blackhole" {
  source              = "../../../common/terraform/modules/lading_http_blackhole"
  type                = var.type
  http-blackhole-toml = file("${path.module}/http_blackhole.toml")
  namespace           = kubernetes_namespace.soak.metadata[0].name
  depends_on   = [module.monitoring]
}
module "tcp-gen" {
  source        = "../../../common/terraform/modules/lading_tcp_gen"
  type          = var.type
  tcp-gen-toml = file("${path.module}/tcp_gen.toml")
  namespace     = kubernetes_namespace.soak.metadata[0].name
  depends_on   = [module.monitoring, module.vector]
}
