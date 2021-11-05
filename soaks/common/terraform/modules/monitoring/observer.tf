data "template_file" "soak-observer" {
  template = file("${path.module}/observer.toml.tpl")
  vars = {
    experiment_name = var.type
    vector_id       = var.vector_image
    query           = "sum(rate((bytes_written[1m])))"
  }
}

resource "kubernetes_config_map" "observer" {
  metadata {
    name      = "observer"
    namespace = kubernetes_namespace.monitoring.metadata[0].name
  }

  data = {
    "observer.toml" = data.template_file.soak-observer.rendered
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
          image             = "ghcr.io/vectordotdev/vector/soak-observer:sha-d108935d14ea7d3405567edb27f55ee03ef7b43d"
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
