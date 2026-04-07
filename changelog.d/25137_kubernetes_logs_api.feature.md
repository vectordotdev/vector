Add new `kubernetes_logs_api` source that collects pod logs via the Kubernetes `pods/log` API endpoint (`GET /api/v1/namespaces/{ns}/pods/{pod}/log`).

Unlike the existing `kubernetes_logs` source, this source does not require hostPath mounts or DaemonSet privileges. It runs as a regular Deployment and needs only namespace-scoped RBAC (`pods/log` get), making it suitable for restricted clusters (OpenShift, hardened GKE/EKS, multi-tenant environments).

authors: git001
