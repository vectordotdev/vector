# Reading a metric event in VRL now includes its value {#vrl-metric-value-root}

## Summary

VRL now exposes the value of a metric event under a field named after its type,
such as `.gauge` or `.distribution`. This changes the result of programs that
already read these paths or the whole event:

- `.gauge` on a gauge metric previously returned `null`. It now returns an
  object containing `.gauge.value`, so `exists(.gauge)` changes from `false` to
  `true`.
- Reading the whole event with `.` includes the value field, so functions such
  as `encode_json(.)` and `keys(.)` include it in their output.
- `unnest` copies the value into each row. With `metric_tag_values: full`,
  `unnest!(.tags.region)` on a distribution copies every sample into every row.

## Migration

If a program only needs the tags, unnest a local copy of them instead of the
event:

#### Old

```coffee
rows = unnest!(.tags.region)
```

#### New

```coffee
tag_view = {"tags": .tags}
rows = unnest!(tag_view.tags.region)
```

To keep the previous output of `encode_json(.)`, copy the event and delete the
field for the metric's type, such as `counter`, `gauge`, or `distribution`. For
a gauge:

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
