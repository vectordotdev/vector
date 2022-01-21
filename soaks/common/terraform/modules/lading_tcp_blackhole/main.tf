resource "kubernetes_config_map" "lading" {
  metadata {
    name      = "lading-tcp-blackhole"
    namespace = var.namespace
  }

  data = {
    "tcp_blackhole.yaml" = var.tcp-blackhole-yaml
  }
}

resource "kubernetes_service" "tcp-blackhole" {
  metadata {
    name      = "tcp-blackhole"
    namespace = var.namespace
  }
  spec {
    selector = {
      app  = "tcp-blackhole"
      type = var.type
    }
    session_affinity = "ClientIP"
    port {
      name        = "ingress"
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

resource "kubernetes_deployment" "tcp-blackhole" {
  metadata {
    name      = "tcp-blackhole"
    namespace = var.namespace
    labels = {
      app  = "tcp-blackhole"
      type = var.type
    }
  }

  spec {
    replicas = 1

    selector {
      match_labels = {
        app  = "tcp-blackhole"
        type = var.type
      }
    }

    template {
      metadata {
        labels = {
          app  = "tcp-blackhole"
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
          image             = var.lading_image
          name              = "tcp-blackhole"
          command           = ["/tcp_blackhole"]

          volume_mount {
            mount_path = "/etc/lading"
            name       = "etc-lading"
            read_only  = true
          }

          resources {
            requests = {
              cpu    = "100m"
              memory = "32Mi"
            }
          }

          port {
            container_port = 8080
            name           = "ingress"
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
