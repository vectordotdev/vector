# Reading a metric event in VRL now includes its value {#vrl-metric-value-root}

## Summary

Reading the whole metric event with `.` in VRL now includes the field that holds the metric's value, such as `gauge` or `distribution`. Programs that encode or iterate over `.` on metrics, such as `encode_json(.)` or `keys(.)`, see the new field. `unnest` on a metric also copies the value into each row, so with `metric_tag_values: full`, `unnest!(.tags.region)` on a distribution copies every sample into every row.

## Migration

If a program only needs the tags, unnest a local copy of them instead of the event:

#### Old

```coffee
rows = unnest!(.tags.region)
```

#### New

```coffee
tag_view = {"tags": .tags}
rows = unnest!(tag_view.tags.region)
```

To keep the previous output of `encode_json(.)`, copy the event and delete the value field for its type:

#### Old

```coffee
.tags.json = encode_json(.)
```

#### New

```coffee
event = .
del(event.gauge)
.tags.json = encode_json(event)
```

authors: JinchengCao
