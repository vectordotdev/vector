resource "kubernetes_config_map" "observer" {
  metadata {
    name      = "observer"
    namespace = kubernetes_namespace.monitoring.metadata[0].name
  }

  data = {
    "observer.yaml" = templatefile("${path.module}/observer.yaml.tpl", {
      experiment_name    = var.experiment_name
      experiment_variant = var.variant
      vector_id          = var.vector_image
    })
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
          image             = "ghcr.io/vectordotdev/vector/soak-observer:sha-3cb06fedb1863956cd2d423c87f6fee13a1f8f40"
          name              = "observer"
          args              = ["--config-path", "/etc/vector/soak/observer.yaml"]

          volume_mount {
            mount_path = "/captures"
            name       = "captures"
          }

          volume_mount {
            mount_path = "/etc/vector/soak"
            name       = "observer-config"
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
        }

        volume {
          name = "captures"
          host_path {
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
