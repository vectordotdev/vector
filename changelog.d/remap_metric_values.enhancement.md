VRL can now read metric values in the `remap` transform, `filter` and `route`
conditions, and unit tests, and can set some of them in the `remap` transform.
Example paths include:

- `.counter.value` and `.gauge.value`, which can also be set on a metric of that
  type, for example, `.gauge.value = 100`
- `.set.values`
- `.distribution.samples`, which can also be replaced with an array of
  `{value, rate}` objects, for example,
  `.distribution.samples = [{"value": 2.5, "rate": 1}]`
- `.aggregated_histogram.buckets`
- `.aggregated_summary.quantiles`
- `.sketch.count`, `.sketch.min`, `.sketch.max`, `.sketch.sum`, and `.sketch.avg`

Other metric value fields are read-only.

authors: JinchengCao
