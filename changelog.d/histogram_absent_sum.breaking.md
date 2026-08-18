# Histograms that report no sum no longer report a sum of zero {#histogram-absent-sum}

## Summary

A histogram's sum is now optional, so a source that reports no sum no longer has one fabricated as
zero. OTLP makes `sum` optional and OpenMetrics only recommends the `_sum` series, so the
`opentelemetry` and `prometheus_*` sources can produce histograms without one. Downstream the sum is
omitted rather than reported as zero: the `prometheus_exporter` and `prometheus_remote_write` sinks
skip the `<name>_sum` series, `influxdb_metrics` omits the `sum` field, `greptimedb_metrics` omits
the `sum` column, and a `lua` transform sees `aggregated_histogram.sum` as `nil`.

## Migration

A `lua` transform that reads `aggregated_histogram.sum` must tolerate `nil`. Unguarded arithmetic
raises `attempt to perform arithmetic on a nil value`, and Vector drops the event.

#### Old

```yaml
transforms:
  histogram_mean:
    type: lua
    inputs: [in]
    version: "2"
    hooks:
      process: |
        function (event, emit)
          local h = event.metric.aggregated_histogram
          event.metric.gauge = { value = h.sum / h.count }
          event.metric.aggregated_histogram = nil
          emit(event)
        end
```

#### New

```yaml
transforms:
  histogram_mean:
    type: lua
    inputs: [in]
    version: "2"
    hooks:
      process: |
        function (event, emit)
          local h = event.metric.aggregated_histogram
          if h.sum ~= nil and h.count > 0 then
            event.metric.gauge = { value = h.sum / h.count }
            event.metric.aggregated_histogram = nil
          end
          emit(event)
        end
```

A VRL program running after `metric_to_log` is affected in a narrower way. `aggregated_histogram.sum`
could already be absent, because the whole `aggregated_histogram` object is absent for other metric
types, so such programs already had to handle the fallible case. What is new is that the field can be
absent while the object is present, so error handling that previously never triggered now can. With
`drop_on_error = true` those events are dropped, so prefer a default:

```coffee
sum = float(.aggregated_histogram.sum) ?? 0.0
```

Consumers of the `prometheus_exporter` and `prometheus_remote_write` sinks may see a `<name>_sum`
series disappear for these histograms. Omitting it is valid under OpenMetrics, which forbids the
series outright for histograms with negative bucket thresholds. Queries deriving an average from
`<name>_sum` should tolerate its absence; `histogram_quantile` never reads it and is unaffected.

authors: gwenaskell
