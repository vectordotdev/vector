The `prometheus_scrape` source has a new `targets` option that can discover and scrape Kubernetes Pods.

A `kubernetes` entry under `targets` watches Pods through the Kubernetes API and scrapes the ones annotated with `prometheus.io/scrape: "true"`. The `prometheus.io/port`, `prometheus.io/path`, `prometheus.io/scheme`, and `prometheus.io/param_<name>` annotations control each target, and the annotation prefix is configurable. `namespaces` and `label_selector` limit which Pods are considered. A `static` entry lists fixed URLs:

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

Metrics from discovered Pods get `namespace`, `pod`, `node`, and `container` tags. Pod labels and annotations can be added as tags through the `pod_label_tags` and `pod_annotation_tags` allowlists, with their keys converted to valid Prometheus label names.

Current limitations:

- Only `role: pod` is supported, and a source can have one `kubernetes` entry.
- Pods without the scrape annotation are not scraped.
- Pending, Succeeded, and Failed Pods, and Pods without an IP address yet, are skipped.
- Builds without the `kubernetes` feature reject `kubernetes` targets.

The existing `endpoints` option still works but is deprecated in favor of `targets`. A source sets one or the other.

authors: leeteng2001 pront JinchengCao
