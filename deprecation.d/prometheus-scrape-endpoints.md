---
what: "`endpoints` option of the `prometheus_scrape` source"
deprecated_since: "0.62.0"
---

The `endpoints` option of the `prometheus_scrape` source, and its `hosts` alias, are deprecated in
favor of the new `targets` option. Existing configurations continue to work, and Vector logs a
warning at startup when `endpoints` is set.

Migrate by replacing:

```yaml
sources:
  my_source:
    type: prometheus_scrape
    endpoints:
      - http://localhost:9090/metrics
```

with:

```yaml
sources:
  my_source:
    type: prometheus_scrape
    targets:
      - static:
          urls:
            - http://localhost:9090/metrics
```
