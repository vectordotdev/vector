Added a new `prometheus_kubernetes_sd` source that auto-discovers and scrapes
Prometheus metrics from Kubernetes Pods using Prometheus-compatible
`prometheus.io/*` annotations.

The source watches Pods via the Kubernetes API and derives scrape targets from
the `prometheus.io/scrape`, `prometheus.io/port`, `prometheus.io/path`,
`prometheus.io/scheme`, and `prometheus.io/param_<name>` annotations. The
annotation prefix is configurable. Discovered targets are scraped on a
configurable interval and emitted with the `namespace`, `pod`, `node`,
`container`, `instance`, and `endpoint` tags. Pod labels and annotations can
be opted-in as additional metric tags via the `pod_label_tags` and
`pod_annotation_tags` allowlists.

The source supports both DaemonSet (set `use_self_node_only: true`) and
single-Deployment topologies, and reuses Vector's standard TLS, auth, and
proxy configuration for scrape requests.
