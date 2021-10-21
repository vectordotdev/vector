resource "kubernetes_namespace" "monitoring" {
  metadata {
    name = "monitoring"
  }
}

resource "kubernetes_cluster_role" "prometheus" {
  metadata {
    name = "prometheus"
  }

  rule {
    api_groups = [""]
    resources  = ["nodes", "nodes/proxy", "services", "endpoints", "pods"]
    verbs      = ["get", "list", "watch"]
  }

  rule {
    api_groups = ["extensions"]
    resources  = ["ingresses"]
    verbs      = ["get", "list", "watch"]
  }

  rule {
    non_resource_urls = ["/metrics"]
    verbs             = ["get"]
  }
}

resource "kubernetes_cluster_role_binding" "prometheus" {
  metadata {
    name = "prometheus"
  }
  role_ref {
    api_group = "rbac.authorization.k8s.io"
    kind      = "ClusterRole"
    name      = "prometheus"
  }
  subject {
    kind      = "ServiceAccount"
    name      = "default"
    namespace = kubernetes_namespace.monitoring.metadata.0.name
  }
}

resource "kubernetes_config_map" "prometheus" {
  metadata {
    name      = "prometheus"
    namespace = kubernetes_namespace.monitoring.metadata[0].name
  }

  data = {
    "prometheus.yml" = "${file("${path.module}/prometheus.yml")}"
  }
}

resource "kubernetes_service" "prometheus" {
  metadata {
    name      = "prometheus"
    namespace = kubernetes_namespace.monitoring.metadata[0].name
  }
  spec {
    selector = {
      app = "prometheus"
    }
    session_affinity = "ClientIP"
    port {
      name        = "prometheus"
      port        = 9090
      target_port = 9090
    }
    type = "NodePort"
  }
}

resource "kubernetes_deployment" "prometheus" {
  metadata {
    name      = "prometheus"
    namespace = kubernetes_namespace.monitoring.metadata[0].name
    labels = {
      app = "prometheus"
    }
  }

  spec {
    replicas = 1

    selector {
      match_labels = {
        app = "prometheus"
      }
    }

    template {
      metadata {
        labels = {
          app = "prometheus"
        }
      }

      spec {
        automount_service_account_token = true
        container {
          image_pull_policy = "IfNotPresent"
          image             = "prom/prometheus:v2.30.3"
          name              = "prometheus"
          args = [
            "--storage.tsdb.retention.time=1h",
            "--config.file=/etc/prometheus/prometheus.yml",
            "--storage.tsdb.path=/prometheus/",
          ]

          volume_mount {
            mount_path = "/etc/prometheus"
            name       = "prometheus-config"
            read_only  = true
          }

          volume_mount {
            mount_path = "/prometheus/"
            name       = "prometheus-storage"
          }

          resources {
            limits = {
              cpu    = "100m"
              memory = "128Mi"
            }
            requests = {
              cpu    = "100m"
              memory = "128Mi"
            }
          }

          port {
            container_port = 9090
            name           = "prometheus"
          }

          # liveness_probe {
          #   http_get {
          #     port = 9598
          #     path = "/metrics"
          #   }
          # }
        }

        volume {
          name = "prometheus-storage"
          empty_dir {}
        }
        volume {
          name = "prometheus-config"
          config_map {
            name = kubernetes_config_map.prometheus.metadata[0].name
          }
        }

      }
    }
  }
}
