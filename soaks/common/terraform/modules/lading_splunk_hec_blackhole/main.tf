resource "kubernetes_config_map" "lading" {
  metadata {
    name      = "lading-splunk-hec-blackhole"
    namespace = var.namespace
  }

  data = {
    "splunk_hec_blackhole.yaml" = var.splunk-hec-blackhole-yaml
  }
}

resource "kubernetes_service" "splunk-hec-blackhole" {
  metadata {
    name      = "splunk-hec-blackhole"
    namespace = var.namespace
  }
  spec {
    selector = {
      app  = "splunk-hec-blackhole"
      type = var.type
    }
    session_affinity = "ClientIP"
    port {
      name        = "prom-export"
      port        = 9090
      target_port = 9090
    }
    port {
      name        = "ingress"
      port        = 8080
      target_port = 8080
    }
    type = "ClusterIP"
  }
}

resource "kubernetes_deployment" "splunk-hec-blackhole" {
  metadata {
    name      = "splunk-hec-blackhole"
    namespace = var.namespace
    labels = {
      app  = "splunk-hec-blackhole"
      type = var.type
    }
  }

  spec {
    replicas = 1

    selector {
      match_labels = {
        app  = "splunk-hec-blackhole"
        type = var.type
      }
    }

    template {
      metadata {
        labels = {
          app  = "splunk-hec-blackhole"
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
          name              = "splunk-hec-blackhole"
          command           = ["/splunk_hec_blackhole"]

          volume_mount {
            mount_path = "/etc/lading"
            name       = "etc-lading"
            read_only  = true
          }

          resources {
            limits = {
              cpu    = "1"
              memory = "512Mi"
            }
            requests = {
              cpu    = "1"
              memory = "512Mi"
            }
          }

          port {
            container_port = 9090
            name           = "prom-export"
          }
          port {
            container_port = 8080
            name           = "ingress"
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
