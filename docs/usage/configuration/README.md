---
description: Vector configuration
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/README.md.erb
-->

# Configuration

![](../../assets/configure.svg)

This section covers configuring Vector and creating [pipelines][docs.pipelines]
like the one shown above. Vector requires only a _single_ [TOML][url.toml]
configurable file, which you can specify via the
[`--config` flag][docs.starting.flags] when [starting][docs.starting] vector:

```bash
vector --config /etc/vector/vector.toml
```

## Example

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
# Set global options
data_dir = "/var/lib/vector"

# Ingest data by tailing one or more files
[sources.apache_logs]
  type         = "file"
  include      = ["/var/log/apache2/*.log"]    # supports globbing
  ignore_older = 86400                         # 1 day

# Structure and parse the data
[transforms.apache_parser]
  inputs       = ["apache_logs"]
  type         = "regex_parser"                # fast/powerful regex
  regex        = '^(?P<host>[w.]+) - (?P<user>[w]+) (?P<bytes_in>[d]+) [(?P<timestamp>.*)] "(?P<method>[w]+) (?P<path>.*)" (?P<status>[d]+) (?P<bytes_out>[d]+)$'

# Sample the data to save on cost
[transforms.apache_sampler]
  inputs       = ["apache_parser"]
  type         = "sampler"
  hash_field   = "request_id"                  # sample _entire_ requests
  rate         = 50                            # only keep 50%

# Send structured data to a short-term storage
[sinks.es_cluster]
  inputs       = ["apache_sampler"]            # only take sampled data
  type         = "elasticsearch"
  host         = "http://79.12.221.222:9200"   # local or external host
  index        = "vector-%Y-%m-%d"             # daily indices

# Send structured data to a cost-effective long-term storage
[sinks.s3_archives]
  inputs       = ["apache_parser"]             # don't sample for S3
  type         = "aws_s3"
  region       = "us-east-1"
  bucket       = "my-log-archives"
  key_prefix   = "date=%Y-%m-%d"               # daily partitions, hive friendly format
  batch_size   = 10000000                      # 10mb uncompressed
  gzip         = true                          # compress final objects
  encoding     = "ndjson"                      # new line delimited JSON
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Global Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **OPTIONAL** | | |
| `data_dir` | `string` | The directory used for persisting Vector state, such as on-disk buffers, file checkpoints, and more. Please make sure the Vector project has write permissions to this dir. See [Data Directory](#data-directory) for more info.<br />`no default` `example: "/var/lib/vector"` |

## Sources

| Name  | Description |
|:------|:------------|
| [**`file`**][docs.file_source] | Ingests data through one or more local files and outputs [`log`][docs.log_event] events. |
| [**`statsd`**][docs.statsd_source] | Ingests data through the StatsD UDP protocol and outputs [`log`][docs.log_event] events. |
| [**`stdin`**][docs.stdin_source] | Ingests data through standard input (STDIN) and outputs [`log`][docs.log_event] events. |
| [**`syslog`**][docs.syslog_source] | Ingests data through the Syslog 5424 protocol and outputs [`log`][docs.log_event] events. |
| [**`tcp`**][docs.tcp_source] | Ingests data through the TCP protocol and outputs [`log`][docs.log_event] events. |
| [**`vector`**][docs.vector_source] | Ingests data through another upstream Vector instance and outputs [`log`][docs.log_event] events. |

[+ request a new source][url.new_source]

## Transforms

| Name  | Description |
|:------|:------------|
| [**`add_fields`**][docs.add_fields_transform] | Accepts [`log`][docs.log_event] events and allows you to add one or more fields. |
| [**`coercer`**][docs.coercer_transform] | Accepts [`log`][docs.log_event] events and allows you to coerce event fields into fixed types. |
| [**`field_filter`**][docs.field_filter_transform] | Accepts [`log`][docs.log_event] and [`metric`][docs.metric_event] events and allows you to filter events by a field's value. |
| [**`grok_parser`**][docs.grok_parser_transform] | Accepts [`log`][docs.log_event] events and allows you to parse a field value with [Grok][url.grok]. |
| [**`json_parser`**][docs.json_parser_transform] | Accepts [`log`][docs.log_event] events and allows you to parse a field value as JSON. |
| [**`log_to_metric`**][docs.log_to_metric_transform] | Accepts [`log`][docs.log_event] events and allows you to convert logs into one or more metrics. |
| [**`lua`**][docs.lua_transform] | Accepts [`log`][docs.log_event] events and allows you to transform events with a full embedded [Lua][url.lua] engine. |
| [**`regex_parser`**][docs.regex_parser_transform] | Accepts [`log`][docs.log_event] events and allows you to parse a field's value with a [Regular Expression][url.regex]. |
| [**`remove_fields`**][docs.remove_fields_transform] | Accepts [`log`][docs.log_event] and [`metric`][docs.metric_event] events and allows you to remove one or more event fields. |
| [**`sampler`**][docs.sampler_transform] | Accepts [`log`][docs.log_event] events and allows you to sample events with a configurable rate. |
| [**`tokenizer`**][docs.tokenizer_transform] | Accepts [`log`][docs.log_event] events and allows you to tokenize a field's value by splitting on white space, ignoring special wrapping characters, and zipping the tokens into ordered field names. |

[+ request a new transform][url.new_transform]

## Sinks

| Name  | Description |
|:------|:------------|
| [**`aws_cloudwatch_logs`**][docs.aws_cloudwatch_logs_sink] | [Batches](#buffers-and-batches) [`log`][docs.log_event] events to [AWS CloudWatch Logs][url.aws_cw_logs] via the [`PutLogEvents` API endpoint](https://docs.aws.amazon.com/AmazonCloudWatchLogs/latest/APIReference/API_PutLogEvents.html). |
| [**`aws_kinesis_streams`**][docs.aws_kinesis_streams_sink] | [Batches](#buffers-and-batches) [`log`][docs.log_event] events to [AWS Kinesis Data Stream][url.aws_kinesis_data_streams] via the [`PutRecords` API endpoint](https://docs.aws.amazon.com/kinesis/latest/APIReference/API_PutRecords.html). |
| [**`aws_s3`**][docs.aws_s3_sink] | [Batches](#buffers-and-batches) [`log`][docs.log_event] events to [AWS S3][url.aws_s3] via the [`PutObject` API endpoint](https://docs.aws.amazon.com/AmazonS3/latest/API/RESTObjectPUT.html). |
| [**`blackhole`**][docs.blackhole_sink] | [Streams](#streaming) [`log`][docs.log_event] and [`metric`][docs.metric_event] events to a blackhole that simply discards data, designed for testing and benchmarking purposes. |
| [**`console`**][docs.console_sink] | [Streams](#streaming) [`log`][docs.log_event] and [`metric`][docs.metric_event] events to the console, `STDOUT` or `STDERR`. |
| [**`elasticsearch`**][docs.elasticsearch_sink] | [Batches](#buffers-and-batches) [`log`][docs.log_event] events to [Elasticsearch][url.elasticsearch] via the [`_bulk` API endpoint](https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-bulk.html). |
| [**`http`**][docs.http_sink] | [Batches](#buffers-and-batches) [`log`][docs.log_event] events to a generic HTTP endpoint. |
| [**`kafka`**][docs.kafka_sink] | [Streams](#streaming) [`log`][docs.log_event] events to [Apache Kafka][url.kafka] via the [Kafka protocol][url.kafka_protocol]. |
| [**`prometheus`**][docs.prometheus_sink] | [Exposes](#exposing-and-scraping) [`metric`][docs.metric_event] events to [Prometheus][url.prometheus] metrics service. |
| [**`splunk_hec`**][docs.splunk_hec_sink] | [Batches](#buffers-and-batches) [`log`][docs.log_event] events to a [Splunk HTTP Event Collector][url.splunk_hec]. |
| [**`tcp`**][docs.tcp_sink] | [Streams](#streaming) [`log`][docs.log_event] events to a TCP connection. |
| [**`vector`**][docs.vector_sink] | [Streams](#streaming) [`log`][docs.log_event] events to another downstream Vector instance. |

[+ request a new sink][url.new_sink]

## How It Works

### Composition

The primary purpose of the configuration file is to compose pipelines. Pipelines
are formed by connecting [sources][docs.sources], [transforms][docs.transforms],
and [sinks][docs.sinks] through the `inputs` option.

Notice in the above example each input references the `id` assigned to a
previous source or transform.

### Config File Location

The location of your Vector configuration file depends on your
[platform][docs.platforms] or [operating system][docs.operating_systems]. For
most Linux based systems the file can be found at `/etc/vector/vector.toml`.

### Data Directory

Vector requires a `data_dir` value for on-disk operations. Currently, the only
operation using this directory are Vector's on-disk buffers. Buffers, by
default, are memory-based, but if you switch them to disk-based you'll need to
specify a `data_dir`.

### Environment Variables

Vector will interpolate environment variables within your configuration file
with the following syntax:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[transforms.add_host]
    type = "add_fields"
    
    [transforms.add_host.fields]
        host = "${HOSTNAME}"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

The entire `${HOSTNAME}` variable will be replaced, hence the requirement of
quotes around the definition.

#### Escaping

You can escape environment variable by preceding them with a `$` character. For
example `$${HOSTNAME}` will be treated _literally_ in the above environment
variable example.

### Format

The Vector configuration file requires the [TOML][url.toml] format for it's
simplicity, explicitness, and relaxed white-space parsing. For more information,
please refer to the excellent [TOML documentation][url.toml].

### Value Types

All TOML values types are supported. For convenience this includes:

* [Strings](https://github.com/toml-lang/toml#string)
* [Integers](https://github.com/toml-lang/toml#integer)
* [Floats](https://github.com/toml-lang/toml#float)
* [Booleans](https://github.com/toml-lang/toml#boolean)
* [Offset Date-Times](https://github.com/toml-lang/toml#offset-date-time)
* [Local Date-Times](https://github.com/toml-lang/toml#local-date-time)
* [Local Dates](https://github.com/toml-lang/toml#local-date)
* [Local Times](https://github.com/toml-lang/toml#local-time)
* [Arrays](https://github.com/toml-lang/toml#array)
* [Tables](https://github.com/toml-lang/toml#table)


[docs.add_fields_transform]: ../../usage/configuration/transforms/add_fields.md
[docs.aws_cloudwatch_logs_sink]: ../../usage/configuration/sinks/aws_cloudwatch_logs.md
[docs.aws_kinesis_streams_sink]: ../../usage/configuration/sinks/aws_kinesis_streams.md
[docs.aws_s3_sink]: ../../usage/configuration/sinks/aws_s3.md
[docs.blackhole_sink]: ../../usage/configuration/sinks/blackhole.md
[docs.coercer_transform]: ../../usage/configuration/transforms/coercer.md
[docs.console_sink]: ../../usage/configuration/sinks/console.md
[docs.elasticsearch_sink]: ../../usage/configuration/sinks/elasticsearch.md
[docs.field_filter_transform]: ../../usage/configuration/transforms/field_filter.md
[docs.file_source]: ../../usage/configuration/sources/file.md
[docs.grok_parser_transform]: ../../usage/configuration/transforms/grok_parser.md
[docs.http_sink]: ../../usage/configuration/sinks/http.md
[docs.json_parser_transform]: ../../usage/configuration/transforms/json_parser.md
[docs.kafka_sink]: ../../usage/configuration/sinks/kafka.md
[docs.log_event]: ../../about/data-model.md#log
[docs.log_to_metric_transform]: ../../usage/configuration/transforms/log_to_metric.md
[docs.lua_transform]: ../../usage/configuration/transforms/lua.md
[docs.metric_event]: ../../about/data-model.md#metric
[docs.operating_systems]: ../../setup/installation/operating-systems
[docs.pipelines]: ../../usage/configuration/README.md#composition
[docs.platforms]: ../../setup/installation/platforms
[docs.prometheus_sink]: ../../usage/configuration/sinks/prometheus.md
[docs.regex_parser_transform]: ../../usage/configuration/transforms/regex_parser.md
[docs.remove_fields_transform]: ../../usage/configuration/transforms/remove_fields.md
[docs.sampler_transform]: ../../usage/configuration/transforms/sampler.md
[docs.sinks]: ../../usage/configuration/sinks
[docs.sources]: ../../usage/configuration/sources
[docs.splunk_hec_sink]: ../../usage/configuration/sinks/splunk_hec.md
[docs.starting.flags]: ../../usage/administration/starting.md#flags
[docs.starting]: ../../usage/administration/starting.md
[docs.statsd_source]: ../../usage/configuration/sources/statsd.md
[docs.stdin_source]: ../../usage/configuration/sources/stdin.md
[docs.syslog_source]: ../../usage/configuration/sources/syslog.md
[docs.tcp_sink]: ../../usage/configuration/sinks/tcp.md
[docs.tcp_source]: ../../usage/configuration/sources/tcp.md
[docs.tokenizer_transform]: ../../usage/configuration/transforms/tokenizer.md
[docs.transforms]: ../../usage/configuration/transforms
[docs.vector_sink]: ../../usage/configuration/sinks/vector.md
[docs.vector_source]: ../../usage/configuration/sources/vector.md
[url.aws_cw_logs]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/WhatIsCloudWatchLogs.html
[url.aws_kinesis_data_streams]: https://aws.amazon.com/kinesis/data-streams/
[url.aws_s3]: https://aws.amazon.com/s3/
[url.elasticsearch]: https://www.elastic.co/products/elasticsearch
[url.grok]: http://grokdebug.herokuapp.com/
[url.kafka]: https://kafka.apache.org/
[url.kafka_protocol]: https://kafka.apache.org/protocol
[url.lua]: https://www.lua.org/
[url.new_sink]: https://github.com/timberio/vector/issues/new?labels=Type%3A+New+Feature
[url.new_source]: https://github.com/timberio/vector/issues/new?labels=Type%3A+New+Feature
[url.new_transform]: https://github.com/timberio/vector/issues/new?labels=Type%3A+New+Feature
[url.prometheus]: https://prometheus.io/
[url.regex]: https://en.wikipedia.org/wiki/Regular_expression
[url.splunk_hec]: http://dev.splunk.com/view/event-collector/SP-CAAAE6M
[url.toml]: https://github.com/toml-lang/toml
