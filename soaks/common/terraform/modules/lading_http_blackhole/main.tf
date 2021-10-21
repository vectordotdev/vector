resource "kubernetes_config_map" "lading" {
  metadata {
    name      = "lading-http-blackhole"
    namespace = var.namespace
  }

  data = {
    "http_blackhole.toml" = var.http-blackhole-toml
  }
}

resource "kubernetes_service" "http-blackhole" {
  metadata {
    name      = "http-blackhole"
    namespace = var.namespace
  }
  spec {
    selector = {
      app  = "http-blackhole"
      type = var.type
    }
    session_affinity = "ClientIP"
    port {
      name        = "datadog-agent"
      port        = 8080
      target_port = 8080
    }
    port {
      name        = "prom-export"
      port        = 9090
      target_port = 9090
    }
    type = "ClusterIP"
  }
}

resource "kubernetes_deployment" "http-blackhole" {
  metadata {
    name      = "http-blackhole"
    namespace = var.namespace
    labels = {
      app  = "http-blackhole"
      type = var.type
    }
  }

  spec {
    replicas = 1

    selector {
      match_labels = {
        app  = "http-blackhole"
        type = var.type
      }
    }

    template {
      metadata {
        labels = {
          app  = "http-blackhole"
          type = var.type
        }
        annotations = {
          "prometheus.io/scrape" = true
          "prometheus.io/port"   = 9090
          "prometheus.io/path"   = "/metrics"
        }
      }

      spec {
        automount_service_account_token = false
        container {
          image_pull_policy = "IfNotPresent"
          image             = "ghcr.io/blt/lading:0.5.0"
          name              = "http-blackhole"
          command           = ["/http_blackhole"]

          volume_mount {
            mount_path = "/etc/lading"
            name       = "etc-lading"
            read_only  = true
          }

          resources {
            limits = {
              cpu    = "100m"
              memory = "32Mi"
            }
            requests = {
              cpu    = "100m"
              memory = "32Mi"
            }
          }

          port {
            container_port = 8080
            name           = "listen"
          }
          port {
            container_port = 9090
            name           = "prom-export"
          }

          liveness_probe {
            http_get {
              port = 9090
              path = "/metrics"
            }
          }
        }

        volume {
          name = "etc-lading"
          config_map {
            name = kubernetes_config_map.lading.metadata[0].name
          }
        }
      }
    }
  }
}
