# RFC 3092 - 2020-08-21 - Apache HTTP Server metrics source

This RFC is to introduce a new metrics source to consume metrics from the
[Apache HTTP Server](https://httpd.apache.org/) (httpd). The high level plan is
to implement a scrapper similar to the existing [prometheus
source](https://vector.dev/docs/reference/sources/prometheus/) that will scrape
the Apache HTTP Server stats endpoint (provided by
[`mod_status`](https://httpd.apache.org/docs/2.4/mod/mod_status.html)) on an
interval and publish metrics to the defined pipeline.

## Scope

This RFC will cover:

- A new source for Apache metrics

This RFC will not cover:

- Generating metrics from Apache logs

## Motivation

Users running httpd want to collect, transform, and forward metrics to better
observe how their webservers are performing.

## Internal Proposal

I expect to largely copy the existing [prometheus
source](https://github.com/vectordotdev/vector/blob/61e806d01d4cc6d2a527b52aa9388d4547f1ebc2/src/sources/prometheus/mod.rs)
and modify it to parse the output of the httpd status page which looks like:

```text
localhost
ServerVersion: Apache/2.4.46 (Unix)
ServerMPM: event
Server Built: Aug  5 2020 23:20:17
CurrentTime: Friday, 21-Aug-2020 18:41:34 UTC
RestartTime: Friday, 21-Aug-2020 18:41:08 UTC
ParentServerConfigGeneration: 1
ParentServerMPMGeneration: 0
ServerUptimeSeconds: 26
ServerUptime: 26 seconds
Load1: 0.00
Load5: 0.03
Load15: 0.03
Total Accesses: 30
Total kBytes: 217
Total Duration: 11
CPUUser: .2
CPUSystem: .02
CPUChildrenUser: 0
CPUChildrenSystem: 0
CPULoad: .846154
Uptime: 26
ReqPerSec: 1.15385
BytesPerSec: 8546.46
BytesPerReq: 7406.93
DurationPerReq: .366667
BusyWorkers: 1
IdleWorkers: 74
Processes: 3
Stopping: 0
BusyWorkers: 1
IdleWorkers: 74
ConnsTotal: 1
ConnsAsyncWriting: 0
ConnsAsyncKeepAlive: 0
ConnsAsyncClosing: 0
Scoreboard: ________________________________________________________W__________________.....................................................................................................................................................................................................................................................................................................................................
```

I'll use this to generate the following metrics:

- `apache_up` (gauge)
- `apache_uptime_seconds_total` (counter)
- `apache_accesses_total` (counter; extended)
- `apache_sent_kilobytes_total` (counter; extended)
- `apache_duration_seconds_total` (counter; extended)
- `apache_cpu_seconds_total{type=(system|user|cpu_children_user|cpu_children_system)}` (gauge; extended)
- `apache_cpu_load` (gauge; extended)
- `apache_workers{state=(busy|idle)}` (gauge)
- `apache_connections{state=(closing|keepalive|writing|total)}` (gauge)
- `apache_scoreboard_waiting{state=(waiting|starting|reading|sending|keepalive|dnslookup|closing|logging|finishing|idle_cleanup|open}` (gauge)

Metrics labeled `extended` are only available if `ExtendedStatus` is enabled
for Apache. This is the default in newer versions (>= 2.4; released 2012), but
purportedly [increases CPU
load](https://www.datadoghq.com/blog/collect-apache-performance-metrics/#a-note-about-extendedstatus)
so some users may turn it off. If it is off, they simply won't have those
metrics published.

I figure we probably don't want metrics for:

- System Load (should be handled by a `cpu` or similar metrics source)

Metrics will be labeled with:

- `endpoint` the full endpoint (sans any basic auth credentials)
- `host` the hostname and port portions of the endpoint

## Doc-level Proposal

Users will be instructed to setup
[`mod_status`](https://httpd.apache.org/docs/2.4/mod/mod_status.html) and
enable
[`ExtendedStatus`](https://httpd.apache.org/docs/2.4/mod/core.html#extendedstatus).

The following additional source configuration will be added:

```toml
[sources.my_source_id]
  type = "apache_metrics" # required
  endpoints = ["http://localhost/server-status?auto"] # required, default
  scrape_interval_secs = 15 # optional, default, seconds
  namespace = "apache" # optional, default, namespace to put metrics under
```

Some possible configuration improvements we could add in the future would be:

- `response_timeout`; to cap request lengths
- `tls`: settings to allow setting specific chains of trust and client certs
- `basic_auth`: to set username/password for use with HTTP basic auth; we'll
  allow this to be set in the URL too which will work for now

But I chose to leave those out for now given the Prometheus source doesn't
support them either. We could add support to both at the same time (see Future
Work section below).

[Datadog's
plugin](https://github.com/DataDog/integrations-core/blob/master/apache/datadog_checks/apache/data/conf.yaml.example)
has numerous more options we could also consider in the future.

The `host` key will be set to the host parsed out of the `endpoint`.

## Rationale

Apache HTTP Server is a fairly common webserver. If we do not support ingesting
metrics from it, it is likely to push people to use another tool to forward
metrics from httpd to the desired sink.

As part of Vector's vision to be the "one tool" for ingesting and shipping
observability data, it makes sense to add as many sources as possible to reduce
the likelihood that a user will not be able to ingest metrics from their tools.

## Prior Art

- [Datadog collection](https://www.datadoghq.com/blog/monitor-apache-web-server-datadog/#set-up-datadogs-apache-integration)
- [Telegraf](https://github.com/influxdata/telegraf/tree/release-1.15/plugins/inputs/apache)

## Drawbacks

- Additional maintenance and integration testing burden of a new source

## Alternatives

### Having users run telegraf and using Vector's prometheus source to scrape it

We could not add the source directly to Vector and instead instruct users to run
Telegraf and point Vector at the exposed Prometheus scrape endpoint. This would
leverage the already supported [telegraf Apache input
plugin](https://github.com/influxdata/telegraf/tree/release-1.15/plugins/inputs/apache)

I decided against this as it would be in contrast with one of the listed
principles of Vector:

> One Tool. All Data. - One simple tool gets your logs, metrics, and traces
> (coming soon) from A to B.

[Vector
principles](https://vector.dev/docs/about/what-is-vector/#who-should-use-vector)

On the same page, it is mentioned that Vector should be a replacement for
Telegraf.

> You SHOULD use Vector to replace Logstash, Fluent*, Telegraf, Beats, or
> similar tools.

If users are already running telegraf though, they could opt for this path.

### Have a generic HTTP scrape source

We could model this as a generic `http_scrape` source that has a `type` (or
`codec`?) that would determine how what type of endpoint it is scraping.

I would err away from this as I think there is some risk that some "HTTP
scrape" endpoints will need source specific configuration. We could do this
later if this does not end up being the case and they really are all the same.

One downside of this is that I think it'd be less discoverable than a
first-class source for each type of endpoint we support scraping.

## Outstanding Questions

- Do we want to apply any metric labels based on the other information
  available via the status page? I could see labeling the `url` at least.
  Answer: label with `host` and `endpoint` as described above.
- Do we want to have one apache_metrics source able to scrape multiple
  endpoints?  Answer: yes, the config has been updated to allow multiple
  endpoints.
- Are there preferences between `apache` or `httpd` for the nomenclature? I
  feel like `apache` is more well-known though `httpd` is more accurate. Answer:
  standardize on `apache`.
- Should the `host` key include the port from the `endpoint` , if any? Or just
  the hostname. Answer: include the port.

## Plan Of Attack

Incremental steps that execute this change. Generally this is in the form of:

- [ ] Submit a PR with the initial sink implementation

## Future Work

### Refactor HTTP-scraping-based sources

I think one thing that would make sense would be to refactor the sources based
on HTTP scraping to share a base similar to how our sinks that rely on `http`
are factored (`splunk_hec`, `http`, `loki`, etc.). This allows them to share
common configuration options for their behavior.

My recommendation is to implement this and the
[`nginx`](https://github.com/vectordotdev/vector/issues/3091) metrics source and
then figure out where the seams our to pull out an `HttpScrapeSource` module
that could be used by this source, the `nginx` source, and the `prometheus`
source.
