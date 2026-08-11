Improve configuration error messages by including the affected field path. For example, this
invalid configuration:

```yaml
sources:
  broken:
    type: demo_logs
    interval: not-a-number
```

now reports `sources.broken: invalid type: string "not-a-number", expected f64`.

authors: pront
