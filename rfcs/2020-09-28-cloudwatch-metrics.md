# RFC #3643 - 2020-09-28 CloudWatch Metrics source RFC

This RFC proposes an implementation for a new metrics source to ingest metrics
from AWS CloudWatch. The proposed implementation is fairly similar to the one
used by telegraf which scrapes `GetMetricData` on a regular interval.

It is probably worth reviewing [Amazon CloudWatch
Concepts](https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/cloudwatch_concepts.html)
before looking at this if you are unfamiliar with CloudWatch.

## Scope

This RFC will simply cover a new `aws_cloudwatch_metrics` source.

## Motivation

Users want to collect and forward metrics from AWS CloudWatch to monitor
infrastructure and deployed services in AWS.

## Internal Proposal

We will add a `aws_cloudwatch_metrics` source that scrapes the AWS API
([`GetMetricData`](https://docs.aws.amazon.com/AmazonCloudWatch/latest/APIReference/API_GetMetricData.html))
on a user-defined interval to collect metrics and forward them through the
user's pipeline.

## Doc-level Proposal

The configuration of the new `aws_cloudwatch_metrics` source will look like:

```toml
[sources.cloudwatch]
  type = "aws_cloudwatch_metrics"
  assume_role = "arn:aws:iam::123456789098:role/my_role" # optional, no default
  endpoints = ["127.0.0.0:5000/path/to/service"] # optional, no default, relevant when regions = []
  regions = ["us-east-1"] # required, required when endpoints unspecified; no default

  period_secs = 300 # period (s) to aggregate metrics over, optional, can be overridden at metric level, default 300
  delay_secs = 300 # delay collection by value (s), used to avoid collecting data that has not fully been processed by CloudWatch, optional, default 300
  interval_secs = 300 # interval to scrape metrics; should be a multiple of "period"; default 300

  metrics_refresh_interval_secs = 900 # interval to refresh available metrics for queried namespaces if globbing or all metrics are used; default 900

  # Request
  request.in_flight_limit = 25 # optional, default, requests
  request.rate_limit_duration_secs = 1 # optional, default, seconds
  request.rate_limit_num = 25 # optional, default
  request.retry_attempts = 18446744073709551615 # optional, default
  request.retry_initial_backoff_secs = 1 # optional, default, seconds
  request.retry_max_duration_secs = 10 # optional, default, seconds
  request.timeout_secs = 30 # optional, default, seconds

  [[sources.cloudwatch.metrics]]
    namespace = "AWS/EC2" # optional; supports globbing
    names = ["EBSReadOps", "EBSReadBytes", "Network*"] # optional; defaults to all metrics in namespace, ["*"], (refreshed on interval); supports globbing
    dimensions.InstanceId = "i-05517fbc2e6124dfb" # optional; supported dimensions differ by namespace and metric; supports globbing
    statistics = [ "average", "sum", "minimum", "maximum", "sample_count" ] # statistics to collect; can also contain extended statistics like p99; default: [ "average", "sum", "minimum", "maximum", "sample_count" ]
    period_secs = 300 # period (s) to aggregate metrics over; defaults to top-level `period` setting; top-level interval should be a multiple of this and any other defined periods
```

We could alternatively model dimensions as another table:

```toml
[[sources.cloudwatch.metrics.dimensions]]
  key = "InstanceId"
  value = "i-05517fbc2e6124dfb"
```

**NOTE** decided against this table representation since we have a few other
examples of vector key/value config that use TOML maps: [`add_tags`
transform](https://vector.dev/docs/reference/transforms/add_tags/) [`http`
sink](https://vector.dev/docs/reference/sinks/http/).

To support:

* globbing metric names
* fetching all metrics for a namespace
* globbing of dimension values

We will refresh and cache the available metrics on an interval
(`metrics_refresh_interval`) for namespaces appearing in the configuration.

A wildcard dimension value (`"*"`) will pull and publish discrete metrics for
each dimension value. This is different than omitting the dimension altogether
which would instead aggregate across the values for the dimension.

Note that dimensions must match the dimensions that the metrics were published
with. For example, if the metric was published two dimensions,
availability-zone and load balancer name, then both dimensions must be
specified to retrieve the metric. CloudWatch won't automatically aggregate
across the other if only one is specified.

This source will use the default AWS credential chain similar to the
`aws_cloudwatch_metrics` sink.

The output (shown here in prometheus format) will publish metrics in the form
of:

```text
<namespace>_<metric_name>_<metric_stat>{<dimension_key>=<dimension_value>*}
```

All metrics will be additionally tagged with the region.

For example:

```text
aws_ec2_cpu_credit_balance_average{instance_id="i-0db9620c0ee32d463",region="us-east-1"} 576
aws_ec2_cpu_credit_balance_maximum{instance_id="i-0db9620c0ee32d463",region="us-east-1"} 576
aws_ec2_cpu_credit_balance_minimum{instance_id="i-0db9620c0ee32d463",region="us-east-1"} 576
aws_ec2_cpu_credit_balance_sample_count{instance_id="i-0db9620c0ee32d463",region="us-east-1"} 1
aws_ec2_cpu_credit_balance_sum{instance_id="i-0db9620c0ee32d463",region="us-east-1"} 576
```

I think we will want to provide example configs that collect common metrics for
various namespaces (like EC2, S3, etc.).

## Rationale

Broadening AWS platform support.

Otherwise, users will need to run `telegraf` or another agent to ingest this
data.

## Prior Art

* [telegraf](https://github.com/influxdata/telegraf/tree/master/plugins/inputs/cloudwatch)
* [prometheus exporter](https://github.com/prometheus/cloudwatch_exporter)
* [filebeat](https://www.elastic.co/guide/en/logstash/current/plugins-inputs-cloudwatch.html)

## Drawbacks

The additional maintenance burden of a new source.

## Alternatives

There is alternate API,
[`GetMetricStatistics'](https://docs.aws.amazon.com/AmazonCloudWatch/latest/APIReference/API_GetMetricStatistics.html)
that we could use, but I think it will result in less overall throughput due to
API rate limit restrictions:

`GetMetricStatistics`

* Allows 400 transactions / sec
* Each transaction can query the 5 standard metrics and up to 10 extended
  metrics for one metric name / dimension combination
* This should allow for: `400 * 5 = 2000` standard metrics / second

`GetMetricData`

* Allows 50 transactions / sec
* Each transaction can query up to 500 metric name / metric stat (like
  `average`) / dimension combination
* This should allow for: `50 * 500 = 25000` standard metrics / second

These are theoretical maxes. They could differ depending on how many metric
statistics are queried for each metric. Querying less than the 5 standard ones
would further advantage `GetMetricData`.

## Outstanding Questions

* Do we care about providing a different rate limit for refreshing available
  metrics? It uses a different API (`ListMetrics`) with different limits than
  `GetMetricData`. Answer: we'll try without for now.

## Plan Of Attack

* [ ] Submit PR with `aws_cloudwatch_metrics` source without any support for
      globbing (which would require listing and caching metrics) and only one
      region
* [ ] Submit follow-up PR allowing for globbing of metric names
* [ ] Submit follow-up PR allowing for globbing of dimension value
* [ ] Submit follow-up PR allowing for globbing of namespace
* [ ] Submit follow-up PR allowing for multiple regions

## Future Work

* Support for [metric
  expressions](https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/using-metric-math.html#metric-math-syntax).
* Filtering metrics by tags. It looks like the logstash input supports this by
  querying for resources first (e.g. instances with a specific tag) and then
  fetching metrics for those instances.
* Backfilling metrics if `vector` is restarted or offline for a period.
* Additional config examples covering common use-cases; basic examples should be
  included with the source documentation.
* Warnings when configured dimensions do not match any metrics in `ListMetrics`.
* Caching `ListMetrics` output across vector restarts to speed up initial start.
