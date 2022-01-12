data "template_file" "soak-observer" {
  template = file("${path.module}/observer.yaml.tpl")
  vars = {
    experiment_name    = var.experiment_name
    experiment_variant = var.variant
    vector_id          = var.vector_image
  }
}

resource "kubernetes_config_map" "observer" {
  metadata {
    name      = "observer"
    namespace = kubernetes_namespace.monitoring.metadata[0].name
  }

  data = {
    "observer.yaml" = data.template_file.soak-observer.rendered
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
          image             = "ghcr.io/vectordotdev/vector/soak-observer:sha-7b2b0d0d6bcaafcd83b3fe636d92d5242d7b550b"
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
