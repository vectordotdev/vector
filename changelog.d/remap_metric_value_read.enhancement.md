VRL can now read the value of a metric event, for example in the `remap` transform, in `filter` and `route` conditions, and in unit tests. The value is under a top-level field named after the metric's type, as in the JSON encoding shown by `vector tap`: `.counter.value`, `.gauge.value`, `.set.values`, `.distribution.samples`, `.aggregated_histogram.buckets`, `.aggregated_summary.quantiles`, and so on. Sketches expose summary statistics directly under `.sketch`: `.sketch.count`, `.sketch.min`, `.sketch.max`, `.sketch.sum`, and `.sketch.avg`. Assigning to these fields or calling `del()` on them doesn't change the metric's value.

Reading the whole metric event with `.` now also includes the value field, so programs that encode or iterate over `.` on metrics, such as `encode_json(.)` or `keys(.)`, see the new field. The same applies to `unnest` on a metric, which copies the rest of the event, now including the value, into each row. With `metric_tag_values: full`, `unnest!(.tags.region)` on a distribution copies every sample into every row. If the program only needs the tags, unnest a local copy instead:

```coffee
tag_view = {"tags": .tags}
rows = unnest!(tag_view.tags.region)
```

authors: JinchengCao
