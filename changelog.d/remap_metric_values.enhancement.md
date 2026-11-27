VRL can now read metric values in the `remap` transform, `filter` and `route`
conditions, and unit tests. Example paths include:

- `.counter.value` and `.gauge.value`
- `.set.values`
- `.distribution.samples`
- `.aggregated_histogram.buckets`
- `.aggregated_summary.quantiles`
- `.sketch.count`, `.sketch.min`, `.sketch.max`, `.sketch.sum`, and `.sketch.avg`

The `remap` transform can also set `.counter.value` or `.gauge.value` on a
metric of that type, for example, `.gauge.value = 100`, and replace
`.distribution.samples` with an array of `{value, rate}` objects, for example,
`.distribution.samples = [{"value": 2.5, "rate": 1}]`. Other metric value
fields are read-only.

authors: JinchengCao
