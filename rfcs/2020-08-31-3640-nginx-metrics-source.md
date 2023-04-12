# RFC 3640 - 2020-08-31 - Nginx HTTP metrics source

This RFC is to introduce a new metrics source to consume metrics from the
[Nginx HTTP Server](https://www.nginx.com/). The high level plan is
to implement a scraper similar to the existing [prometheus
source](https://vector.dev/docs/reference/sources/prometheus/) that will scrape
the Nginx HTTP Server stats endpoint (provided by
[`stub_status`](https://nginx.org/en/docs/http/ngx_http_stub_status_module.html#stub_status)) on an
interval and publish metrics to the defined pipeline.

## Scope

This RFC will cover:

- A new source for Nginx metrics

This RFC will not cover:

- Generating metrics from Nginx logs

## Motivation

Users running Nginx want to collect, transform, and forward metrics to better
observe how their webserver's are performing.

## Internal Proposal

I expect to largely copy the existing [prometheus
source](https://github.com/vectordotdev/vector/blob/61e806d01d4cc6d2a527b52aa9388d4547f1ebc2/src/sources/prometheus/mod.rs)
and modify it to parse the output of the Nginx stub status page which looks like:

```text
Active connections: 1
server accepts handled requests
 1 1 1
Reading: 0 Writing: 1 Waiting: 0
```

The breakdown of this output:

- Active connections
  - The current number of active client connections including Waiting connections.
- accepts
  - The total number of accepted client connections.
- handled
  - The total number of handled connections. Generally, the parameter value is the same as accepts unless some resource limits have been reached (for example, the worker_connections limit).
- requests
  - The total number of client requests.
- Reading
  - The current number of connections where nginx is reading the request header.
- Writing
  - The current number of connections where nginx is writing the response back to the client.
- Waiting
  - The current number of idle client connections waiting for a request

We'll use this to generate the following metrics:

- `nginx_up` (gauge)
- `nginx_connections_active` (gauge)
- `nginx_connections_accepted_total` (counter)
- `nginx_connections_reading` (gauge)
- `nginx_connections_waiting` (gauge)
- `nginx_connections_writing` (gauge)
- `nginx_http_requests_total` (counter)

Metrics will be labeled with:

- `endpoint` the full endpoint (sans any basic authentication credentials)
- `host` the host name and port portions of the endpoint

## Doc-level Proposal

Users will be instructed to setup
[`stub_status`](https://nginx.org/en/docs/http/ngx_http_stub_status_module.html#stub_status)
The following additional source configuration will be added:

```toml
[sources.my_source_id]
  type = "nginx_metrics" # required
  endpoints = ["http://localhost/basic_status"] # required, default
  scrape_interval_secs = 15 # optional, default, seconds
  namespace = "nginx" # optional, default, namespace to put metrics under
```

Some possible configuration improvements we could add in the future would be:

- `response_timeout`; to cap request lengths
- `tls`: settings to allow setting specific chains of trust and client certs
- `basic_auth`: to set username/password for use with HTTP basic auth; we'll
  allow this to be set in the URL too which will work for now

The `host` key will be set to the host parsed out of the `endpoint`.

## Rationale

Nginx HTTP server is a common web server. If we do not support ingesting
metrics from it, it is likely to push people to use another tool to forward
metrics from Nginx to the desired sink.

As part of Vector's vision to be the "one tool" for ingesting and shipping
observability data, it makes sense to add as many sources as possible to reduce
the likelihood that a user will not be able to ingest metrics from their tools.

## Prior Art

- [Nginx's Prometheus exporter](https://github.com/nginxinc/nginx-prometheus-exporter)
- [Telegraf](https://github.com/influxdata/telegraf/tree/release-1.15/plugins/inputs/nginx)
- [DataDog](https://www.datadoghq.com/blog/how-to-collect-nginx-metrics/)
- [Collectd Nginx plugin](https://collectd.org/documentation/manpages/collectd.conf.5.shtml#plugin_nginx)
- [New Relic Nginx](https://github.com/nginxinc/new-relic-agent)

## Drawbacks

- Additional maintenance and integration testing burden of a new source

## Alternatives

### Having users run Telegraf and using Vector's Prometheus source to scrape it

We could not add the source directly to Vector and instead instruct users to run
Telegraf and point Vector at the exposed Prometheus scrape endpoint. This would
leverage the already supported [telegraf Nginx input
plugin](https://github.com/influxdata/telegraf/tree/release-1.15/plugins/inputs/nginx)

Or someone could use the Prometheus Nginx exporter directly and the `prometheus` sink.

We decided against this as it would be in contrast with one of the listed
principles of Vector:

> One Tool. All Data. - One simple tool gets your logs, metrics, and traces
> (coming soon) from A to B.

[Vector
principles](https://vector.dev/docs/about/what-is-vector/#who-should-use-vector)

On the same page, it is mentioned that Vector should be a replacement for
Telegraf.

> You SHOULD use Vector to replace Logstash, Fluent*, Telegraf, Beats, or
> similar tools.

If users are already running Telegraf though, they could opt for this path.

## Outstanding Questions

- None

## Plan Of Attack

Incremental steps that execute this change. Generally this is in the form of:

- [ ] Submit a PR with the initial sink implementation

## Future Work

- Nginx Plus support
  - https://github.com/influxdata/telegraf/tree/release-1.15/plugins/inputs/nginx_plus
  - https://github.com/influxdata/telegraf/tree/release-1.15/plugins/inputs/nginx_plus_api
- Support for:
  - nginx_sts
  - nginx_upstream_check
  - nginx_vts

### Refactor HTTP-scraping-based sources

I think one thing that would make sense would be to refactor the sources based
on HTTP scraping to share a base similar to how our sinks that rely on `http`
are factored (`splunk_hec`, `http`, `loki`, etc.). This allows them to share
common configuration options for their behavior.
