VRL can now read metric values in the `remap` transform, `filter` and `route`
conditions, and unit tests. Example paths include:

- `.counter.value` and `.gauge.value`
- `.set.values`
- `.distribution.samples`
- `.aggregated_histogram.buckets`
- `.aggregated_summary.quantiles`
- `.sketch.count`, `.sketch.min`, `.sketch.max`, `.sketch.sum`, and `.sketch.avg`

These fields are read-only: assignments and `del()` leave the metric's value unchanged.

authors: JinchengCao
