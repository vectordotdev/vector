# RFC 3684 - 2020-09-02 - First-class Metric Namespaces

This RFC proposes making `namespace` a first-class field on the internal
`Metric` type to allow it to be set in sources, manipulated in transforms, and
used by sinks.

## Scope

This RFC will cover:

- Separating the `namespace` of metrics into a separate field on `Metric`

## Motivation

As we add `metric` sources like
[`apache_metrics`](https://github.com/vectordotdev/vector/blob/master/rfcs/2020-08-21-3092-apache-metrics)
and
[`postgresql_metrics`](https://github.com/vectordotdev/vector/blob/master/rfcs/2020-08-27-3603-postgres-metrics.md),
that set their own namespaces (defaulting to `apache` and `postgresql`), it is
becoming more clear that we may want to maintain the `namespace` separate from
the metric name to allow for:

- simple manipulation in transforms (e.g. as a filter)
- to be sent as a separate field for sinks that require it (e.g.
  `aws_cloudwatch_metrics`)
- to be formatted differently depending on sink convention (e.g. it looks like
  [NewRelic prefers
  `namespace.name`](https://docs.newrelic.com/docs/telemetry-data-platform/get-data/apis/report-metrics-metric-api)
  for metrics).

I believe the current implementation proposals for these metrics sources will
simply prefix the name as the `prometheus` and `statsd` sinks do, but this will
be difficult to use with the `aws_cloudwatch_metrics` which requires the
`namespace` as a separate field for the AWS API calls.

Additionally, I think separating it will allow it to be more useful in
transforms (users could currently emulate this by prefix matches of the metric
name).

## Internal Proposal

Add `namespace` to
[`Metric`](https://github.com/vectordotdev/vector/blob/75844bc0f67d24ad1b54bfa130d074810ad2aa50/src/event/metric.rs#L10-L17):

```rust
pub struct Metric {
  pub name: String,
  pub namespace: Option<String>, // added
  pub timestamp: Option<DateTime<Utc>>,
  pub tags: Option<BTreeMap<String, String>>,
  pub kind: MetricKind,
  #[serde(flatten)]
  pub value: MetricValue,
}
```

Metric sources can then optionally assign a `namespace` for the metric.

For example, the upcoming [MongoDB
source](https://github.com/vectordotdev/vector/pull/3681) would set this to
`mongodb`.

Sinks can then decide what to do with this prefix. For example, the
`prometheus` sink would simply the metric name with it, but
`aws_cloudwatch_metrics` would use it as the `Namespace` field in
[`PutMetricData`](https://docs.aws.amazon.com/AmazonCloudWatch/latest/APIReference/API_PutMetricData.html)
requests.

## Doc-level Proposal

A pipeline might look like:

```toml
[sources.my_source_id]
  type = "apache_metrics"
  endpoints = ["http://localhost/server-status?auto"]
  namespace = "apache"

[transforms.my_transform_id]
  # General
  type = "lua" # required
  inputs = ["my_source_id"] # required
  version = "2" # required

  # Hooks
  hooks.process = """
  function (event, emit)
    if event.metric.namespace == "apache" then
      -- do something
    end

    emit(event)
  end
  """

[sinks.prometheus]
  type = "prometheus"
  inputs = ["my_transform_id"]
  address = "0.0.0.0:9598"
  namespace = ""

[sinks.cloudwatch]
  type = "aws_cloudwatch_metrics"
  inputs = ["my_transform_id"]
  namespace = ""
  region = "us-east-1"
```

Where the `prometheus` sink would simply output metrics with name prefixed by
`apache_` and `aws_cloudwatch_metrics` would use it as the separate `Namespace`
field in AWS API calls.

Once [Make the `namespace` option on metrics sinks optional #3609](
https://github.com/vectordotdev/vector/issues/3609) is done. The sinks could look
something like:

```toml
[sinks.my_sink_id]
  type = "prometheus"
  inputs = ["my_transform_id"]
  address = "0.0.0.0:9598"
  default_namespace = "unknown"
```

Where a namespace could be set for any metrics that do not already have one.

## Rationale

Currently, I don't think there is a way to tell if a metric already has a
namespace to avoid setting an additional one in sinks the require it.

## Prior Art

- [telegraf](https://github.com/influxdata/telegraf/blob/b5fafb4c957b55701738f3ae78da4f54ffdec965/metric/metric.go#L12-L20).
  They model it a bit differently with a `metric` having a number of fields
  where the `name` is what we would call the `namespace` and `fields` is what we
  would generate individual metrics for.

## Drawbacks

- Not all metric sources may have the concept of a "namespace" so we'll need to
  figure out what do with it for those cases. It think prefixing it as
  `prometheus` does would be a reasonable default.
- This namespace concept may be confusing to users if none of their sinks or
  sources use it

## Alternatives

### Model a metric as a set of measurements

We could opt to model metrics closer to [how Telegraf does
it](https://github.com/influxdata/telegraf/blob/b5fafb4c957b55701738f3ae78da4f54ffdec965/metric/metric.go#L12-L20)
where we would encode all of the metrics for a given source as one metric with
a set of fields.

I didn't closely consider this option given that the proposed option seems
reasonable and is a smaller change to the data model.

## Outstanding Questions

- Do we want to have the `prometheus` source parse the `namespace` out of
  metrics it scrapes? The [naming
  conventions](https://prometheus.io/docs/practices/naming/) suggest that all
  metrics should start with one word describing the domain (or namespace)
  followed by a `_` but there is requirement that prometheus endpoints satisfy
  this. We could make it optional directive on the source to control parsing
  metric namespaces.

## Plan Of Attack

Incremental steps that execute this change. Generally this is in the form of:

- [ ] Submit a PR with `namespace` modeled as a first-class field

## Future Work

None
