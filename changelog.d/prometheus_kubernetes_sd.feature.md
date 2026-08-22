Added a `targets` field to `prometheus_scrape` that supports auto-discovering and
scraping Prometheus metrics from Kubernetes Pods using Prometheus-compatible
`prometheus.io/*` annotations.

The new `targets` field accepts a `kubernetes` block that watches Pods via the
Kubernetes API and derives scrape targets from the `prometheus.io/scrape`,
`prometheus.io/port`, `prometheus.io/path`, `prometheus.io/scheme`, and
`prometheus.io/param_<name>` annotations. The annotation prefix is configurable.

The existing `endpoints` field is deprecated but continues to work for backwards
compatibility. Only one of `endpoints` or `targets` can be specified.

Static scrape targets can also be specified under `targets` via the `static` block:

```yaml
type: prometheus_scrape
targets:
  - static:
      urls:
        - http://localhost:9090/metrics
  - kubernetes:
      role: pod
      namespaces: [default, monitoring]
```

Discovered targets are scraped on the source's configured interval and emitted
with `namespace`, `pod`, `node`, `container`, `instance`, and `endpoint` tags.
Pod labels and annotations can be opted-in as additional metric tags via the
`pod_label_tags` and `pod_annotation_tags` allowlists.

authors: leeteng2001
