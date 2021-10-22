resource "kubernetes_config_map" "observer" {
  metadata {
    name      = "observer"
    namespace = kubernetes_namespace.monitoring.metadata[0].name
  }

  data = {
    "observer.toml" = var.observer-toml
  }
}

resource "kubernetes_deployment" "observer" {
  metadata {
    name      = "observer"
    namespace = kubernetes_namespace.monitoring.metadata[0].name
    labels = {
      app = "observer"
    }
  }

  spec {
    replicas = 1

    selector {
      match_labels = {
        app = "observer"
      }
    }

    template {
      metadata {
        labels = {
          app = "observer"
        }
      }

      spec {
        automount_service_account_token = false
        container {
          image_pull_policy = "IfNotPresent"
          image             = "ghcr.io/vectordotdev/vector/soak-observer:sha-55446e9592d5105260cce878489121f9daf20bef"
          name              = "observer"

          volume_mount {
            mount_path = "/captures"
            name = "captures"
          }

          volume_mount {
            mount_path = "/etc/vector/soak"
            name       = "observer-config"
            read_only  = true
          }

          resources {
            limits = {
              cpu    = "10m"
              memory = "8Mi"
            }
            requests = {
              cpu    = "10m"
              memory = "8Mi"
            }
          }
        }

        volume {
          name = "captures"
          host_path = {
            path = "/captures"
          }
        }
        volume {
          name = "observer-config"
          config_map {
            name = kubernetes_config_map.observer.metadata[0].name
          }
        }
      }
    }
  }
}
