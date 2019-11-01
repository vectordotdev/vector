---
title: "elasticsearch sink" 
sidebar_label: "elasticsearch"
---

The `elasticsearch` sink [batches](#buffers-and-batches) [`log`][docs.data-model.log] events to [Elasticsearch][urls.elasticsearch] via the [`_bulk` API endpoint](https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-bulk.html).

## Example

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

<Tabs
  defaultValue="simple"
  values={[
    { label: 'Simple', value: 'simple', },
    { label: 'Advanced', value: 'advanced', },
  ]
}>
<TabItem value="simple">

```coffeescript
[sinks.my_sink_id]
  type = "elasticsearch" # enum
  inputs = ["my-source-id"]
  host = "http://10.24.32.122:9000"
```

</TabItem>
<TabItem value="advanced">

```coffeescript
[sinks.my_sink_id]
  # REQUIRED - General
  type = "elasticsearch" # enum
  inputs = ["my-source-id"]
  host = "http://10.24.32.122:9000"
  
  # OPTIONAL - General
  doc_type = "_doc" # default
  healthcheck = true # default
  index = "vector-%Y-%m-%d" # default
  provider = "default" # default, enum
  region = "us-east-1" # no default
  
  # OPTIONAL - Batching
  batch_size = 10490000 # default, bytes
  batch_timeout = 1 # default, seconds
  
  # OPTIONAL - Requests
  rate_limit_duration = 1 # default, seconds
  rate_limit_num = 5 # default
  request_in_flight_limit = 5 # default
  request_timeout_secs = 60 # default, seconds
  retry_attempts = 5 # default
  retry_backoff_secs = 5 # default, seconds
  
  # OPTIONAL - Basic auth
  [sinks.my_sink_id.basic_auth]
    password = "password"
    user = "username"
  
  # OPTIONAL - Buffer
  [sinks.my_sink_id.buffer]
    type = "memory" # default, enum
    max_size = 104900000 # no default, bytes, relevant when type = "disk"
    num_items = 500 # default, events, relevant when type = "memory"
    when_full = "block" # default, enum
  
  # OPTIONAL - Headers
  [sinks.my_sink_id.headers]
    X-Powered-By = "Vector" # example
  
  # OPTIONAL - Query
  [sinks.my_sink_id.query]
    X-Powered-By = "Vector" # example
  
  # OPTIONAL - Tls
  [sinks.my_sink_id.tls]
    ca_path = "/path/to/certificate_authority.crt" # no default
    crt_path = "/path/to/host_certificate.crt" # no default
    key_pass = "PassWord1" # no default
    key_path = "/path/to/host_certificate.key" # no default
    verify_certificate = true # default
    verify_hostname = true # default
```

</TabItem>

</Tabs>

You can learn more

## Options

import Option from '@site/src/components/Option';
import Options from '@site/src/components/Options';

<Options filters={true}>


<Option
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"basic_auth"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"table"}
  unit={null}>

### basic_auth

Options for basic authentication.

<Options filters={false}>


<Option
  defaultValue={null}
  enumValues={null}
  examples={["password","${PASSWORD_ENV_VAR}"]}
  name={"password"}
  nullable={false}
  path={"basic_auth"}
  relevantWhen={null}
  required={true}
  simple={true}
  type={"string"}
  unit={null}>

#### password

The basic authentication password.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={["username"]}
  name={"user"}
  nullable={false}
  path={"basic_auth"}
  relevantWhen={null}
  required={true}
  simple={true}
  type={"string"}
  unit={null}>

#### user

The basic authentication user name.


</Option>


</Options>

</Option>


<Option
  defaultValue={10490000}
  enumValues={null}
  examples={[10490000]}
  name={"batch_size"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={"bytes"}>

### batch_size

The maximum size of a batch before it is flushed. See [Buffers & Batches](#buffers-batches) for more info.


</Option>


<Option
  defaultValue={1}
  enumValues={null}
  examples={[1]}
  name={"batch_timeout"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={"seconds"}>

### batch_timeout

The maximum age of a batch before it is flushed. See [Buffers & Batches](#buffers-batches) for more info.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"buffer"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"table"}
  unit={null}>

### buffer

Configures the sink specific buffer.

<Options filters={false}>


<Option
  defaultValue={"memory"}
  enumValues={{"memory":"Stores the sink's buffer in memory. This is more performant (~3x), but less durable. Data will be lost if Vector is restarted abruptly.","disk":"Stores the sink's buffer on disk. This is less performance (~3x),  but durable. Data will not be lost between restarts."}}
  examples={["memory","disk"]}
  name={"type"}
  nullable={false}
  path={"buffer"}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

#### type

The buffer's type / location. `disk` buffers are persistent and will be retained between restarts.


</Option>


<Option
  defaultValue={"block"}
  enumValues={{"block":"Applies back pressure when the buffer is full. This prevents data loss, but will cause data to pile up on the edge.","drop_newest":"Drops new data as it's received. This data is lost. This should be used when performance is the highest priority."}}
  examples={["block","drop_newest"]}
  name={"when_full"}
  nullable={false}
  path={"buffer"}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

#### when_full

The behavior when the buffer becomes full.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={[104900000]}
  name={"max_size"}
  nullable={true}
  path={"buffer"}
  relevantWhen={{"type":"disk"}}
  required={false}
  simple={false}
  type={"int"}
  unit={"bytes"}>

#### max_size

The maximum size of the buffer on the disk.


</Option>


<Option
  defaultValue={500}
  enumValues={null}
  examples={[500]}
  name={"num_items"}
  nullable={true}
  path={"buffer"}
  relevantWhen={{"type":"memory"}}
  required={false}
  simple={false}
  type={"int"}
  unit={"events"}>

#### num_items

The maximum number of [events][docs.event] allowed in the buffer.


</Option>


</Options>

</Option>


<Option
  defaultValue={"_doc"}
  enumValues={null}
  examples={["_doc"]}
  name={"doc_type"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

### doc_type

The `doc_type` for your index data. This is only relevant for Elasticsearch <= 6.X. If you are using >= 7.0 you do not need to set this option since Elasticsearch has removed it.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"headers"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"table"}
  unit={null}>

### headers

Options for custom headers.

<Options filters={false}>


<Option
  defaultValue={null}
  enumValues={null}
  examples={[{"name":"X-Powered-By","value":"Vector"}]}
  name={"*"}
  nullable={false}
  path={"headers"}
  relevantWhen={null}
  required={true}
  simple={true}
  type={"string"}
  unit={null}>

#### *

A custom header to be added to each outgoing Elasticsearch request.


</Option>


</Options>

</Option>


<Option
  defaultValue={true}
  enumValues={null}
  examples={[true,false]}
  name={"healthcheck"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"bool"}
  unit={null}>

### healthcheck

Enables/disables the sink healthcheck upon start. See [Health Checks](#health-checks) for more info.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={["http://10.24.32.122:9000"]}
  name={"host"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  simple={true}
  type={"string"}
  unit={null}>

### host

The host of your Elasticsearch cluster. This should be the full URL as shown in the example.


</Option>


<Option
  defaultValue={"vector-%F"}
  enumValues={null}
  examples={["vector-%Y-%m-%d","application-{{ application_id }}-%Y-%m-%d"]}
  name={"index"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

### index

Index name to write events to. See [Template Syntax](#template-syntax) for more info.


</Option>


<Option
  defaultValue={"default"}
  enumValues={{"default":"A generic Elasticsearch provider.","aws":"The [AWS Elasticsearch Service][urls.aws_elasticsearch]."}}
  examples={["default","aws"]}
  name={"provider"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

### provider

The provider of the Elasticsearch service. This is used to properly authenticate with the Elasticsearch cluster. For example, authentication for [AWS Elasticsearch Service][urls.aws_elasticsearch] requires that we obtain AWS credentials to properly sign the request.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"query"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"table"}
  unit={null}>

### query

Custom parameters to Elasticsearch query string.

<Options filters={false}>


<Option
  defaultValue={null}
  enumValues={null}
  examples={[{"name":"X-Powered-By","value":"Vector"}]}
  name={"*"}
  nullable={false}
  path={"query"}
  relevantWhen={null}
  required={true}
  simple={true}
  type={"string"}
  unit={null}>

#### *

A custom parameter to be added to each Elasticsearch request.


</Option>


</Options>

</Option>


<Option
  defaultValue={1}
  enumValues={null}
  examples={[1]}
  name={"rate_limit_duration"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={"seconds"}>

### rate_limit_duration

The window used for the `request_rate_limit_num` option See [Rate Limits](#rate-limits) for more info.


</Option>


<Option
  defaultValue={5}
  enumValues={null}
  examples={[5]}
  name={"rate_limit_num"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={null}>

### rate_limit_num

The maximum number of requests allowed within the `rate_limit_duration` window. See [Rate Limits](#rate-limits) for more info.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={["us-east-1"]}
  name={"region"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

### region

When using the AWS provider, the [AWS region][urls.aws_elasticsearch_regions] of the target Elasticsearch instance.


</Option>


<Option
  defaultValue={5}
  enumValues={null}
  examples={[5]}
  name={"request_in_flight_limit"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={null}>

### request_in_flight_limit

The maximum number of in-flight requests allowed at any given time. See [Rate Limits](#rate-limits) for more info.


</Option>


<Option
  defaultValue={60}
  enumValues={null}
  examples={[60]}
  name={"request_timeout_secs"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={"seconds"}>

### request_timeout_secs

The maximum time a request can take before being aborted. It is highly recommended that you do not lower value below the service's internal timeout, as this could create orphaned requests, pile on retries, and result in deuplicate data downstream.


</Option>


<Option
  defaultValue={5}
  enumValues={null}
  examples={[5]}
  name={"retry_attempts"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={null}>

### retry_attempts

The maximum number of retries to make for failed requests. See [Retry Policy](#retry-policy) for more info.


</Option>


<Option
  defaultValue={5}
  enumValues={null}
  examples={[5]}
  name={"retry_backoff_secs"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={"seconds"}>

### retry_backoff_secs

The amount of time to wait before attempting a failed request again. See [Retry Policy](#retry-policy) for more info.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"tls"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"table"}
  unit={null}>

### tls

Configures the TLS options for connections from this sink.

<Options filters={false}>


<Option
  defaultValue={null}
  enumValues={null}
  examples={["/path/to/certificate_authority.crt"]}
  name={"ca_path"}
  nullable={true}
  path={"tls"}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

#### ca_path

Absolute path to an additional CA certificate file, in DER or PEM format (X.509).


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={["/path/to/host_certificate.crt"]}
  name={"crt_path"}
  nullable={true}
  path={"tls"}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

#### crt_path

Absolute path to a certificate file used to identify this connection, in DER or PEM format (X.509) or PKCS#12. If this is set and is not a PKCS#12 archive, `key_path` must also be set.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={["/path/to/host_certificate.key"]}
  name={"key_path"}
  nullable={true}
  path={"tls"}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

#### key_path

Absolute path to a certificate key file used to identify this connection, in DER or PEM format (PKCS#8). If this is set, `crt_path` must also be set.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={["PassWord1"]}
  name={"key_pass"}
  nullable={true}
  path={"tls"}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

#### key_pass

Pass phrase used to unlock the encrypted key file. This has no effect unless `key_pass` above is set.


</Option>


<Option
  defaultValue={true}
  enumValues={null}
  examples={[true,false]}
  name={"verify_certificate"}
  nullable={true}
  path={"tls"}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"bool"}
  unit={null}>

#### verify_certificate

If `true` (the default), Vector will validate the TLS certificate of the remote host. Do NOT set this to `false` unless you understand the risks of not verifying the remote certificate.


</Option>


<Option
  defaultValue={true}
  enumValues={null}
  examples={[true,false]}
  name={"verify_hostname"}
  nullable={true}
  path={"tls"}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"bool"}
  unit={null}>

#### verify_hostname

If `true` (the default), Vector will validate the configured remote host name against the remote host's TLS certificate. Do NOT set this to `false` unless you understand the risks of not verifying the remote hostname.


</Option>


</Options>

</Option>


</Options>

## Input/Output

The `elasticsearch` sink batches [`log`][docs.data-model.log] up to the `batch_size` or `batch_timeout` options. When flushed, Vector will write to [Elasticsearch][urls.elasticsearch] via the [`_bulk` API endpoint](https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-bulk.html). The encoding is dictated by the `encoding` option. For example:

```http
POST <host>/_bulk HTTP/1.1
Host: <host>
Content-Type: application/x-ndjson
Content-Length: 654

{ "index" : { "_index" : "<index>" } }
{"timestamp": 1557932537, "message": "GET /roi/evolve/embrace/transparent", "host": "Stracke8362", "process_id": 914, "remote_addr": "30.163.82.140", "response_code": 504, "bytes": 29763} 
{ "index" : { "_index" : "<index>" } }
{"timestamp": 1557933548, "message": "PUT /value-added/b2b", "host": "Wiza2458", "process_id": 775, "remote_addr": "30.163.82.140", "response_code": 503, "bytes": 9468}
{ "index" : { "_index" : "<index>" } }
{"timestamp": 1557933742, "message": "DELETE /reinvent/interfaces", "host": "Herman3087", "process_id": 775, "remote_addr": "43.246.221.247", "response_code": 503, "bytes": 9700}
```

## How It Works

### Buffers & Batches

![][assets.sink-flow-serial]

The `elasticsearch` sink buffers & batches data as
shown in the diagram above. You'll notice that Vector treats these concepts
differently, instead of treating them as global concepts, Vector treats them
as sink specific concepts. This isolates sinks, ensuring services disruptions
are contained and [delivery guarantees][docs.guarantees] are honored.

*Batches* are flushed when 1 of 2 conditions are met:

1. The batch age meets or exceeds the configured `batch_timeout` (default: `1 seconds`).
2. The batch size meets or exceeds the configured `batch_size` (default: `10490000 bytes`).

*Buffers* are controlled via the [`buffer.*`](#buffer) options.

### Delivery Guarantee

Due to the nature of this component, it offers a
[**best effort** delivery guarantee][docs.guarantees#best-effort-delivery].

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.

### Health Checks

Health checks ensure that the downstream service is accessible and ready to
accept data. This check is performed upon sink initialization.

If the health check fails an error will be logged and Vector will proceed to
start. If you'd like to exit immediately upon health check failure, you can
pass the `--require-healthy` flag:

```bash
vector --config /etc/vector/vector.toml --require-healthy
```

And finally, if you'd like to disable health checks entirely for this sink
you can set the `healthcheck` option to `false`.

### Nested Documents

Vector will explode events into nested documents before writing them to
Elasticsearch. Vector assumes keys with a . delimit nested fields. You can read
more about how Vector handles nested documents in the [Data Model
document][docs.data_model].

### Rate Limits

Vector offers a few levers to control the rate and volume of requests to the
downstream service. Start with the `rate_limit_duration` and `rate_limit_num`
options to ensure Vector does not exceed the specified number of requests in
the specified window. You can further control the pace at which this window is
saturated with the `request_in_flight_limit` option, which will guarantee no
more than the specified number of requests are in-flight at any given time.

Please note, Vector's defaults are carefully chosen and it should be rare that
you need to adjust these. If you found a good reason to do so please share it
with the Vector team by [opening an issie][urls.new_elasticsearch_sink_issue].

### Retry Policy

Vector will retry failed requests (status == `429`, >= `500`, and != `501`).
Other responses will _not_ be retried. You can control the number of retry
attempts and backoff rate with the `retry_attempts` and `retry_backoff_secs` options.

### Template Syntax

The `index` options
support [Vector's template syntax][docs.configuration#template-syntax],
enabling dynamic values derived from the event's data. This syntax accepts
[strftime specifiers][urls.strftime_specifiers] as well as the
`{{ field_name }}` syntax for accessing event fields. For example:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[sinks.my_elasticsearch_sink_id]
  # ...
  index = "vector-%Y-%m-%d"
  index = "application-{{ application_id }}-%Y-%m-%d"
  # ...
```
{% endcode-tabs-item %}
{% endcode-tabs %}

You can read more about the complete syntax in the
[template syntax section][docs.configuration#template-syntax].

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring#logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `elasticsearch_sink` issues][urls.elasticsearch_sink_issues].
2. If encountered a bug, please [file a bug report][urls.new_elasticsearch_sink_bug].
3. If encountered a missing feature, please [file a feature request][urls.new_elasticsearch_sink_enhancement].
4. If you need help, [join our chat/forum community][urls.vector_chat]. You can post a question and search previous questions.

## Resources

* [**Issues**][urls.elasticsearch_sink_issues] - [enhancements][urls.elasticsearch_sink_enhancements] - [bugs][urls.elasticsearch_sink_bugs]
* [**Source code**][urls.elasticsearch_sink_source]


[assets.sink-flow-serial]: ../../../assets/sink-flow-serial.svg
[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.configuration#template-syntax]: ../../../usage/configuration#template-syntax
[docs.data-model.log]: ../../../about/data-model/log.md
[docs.data_model]: ../../../about/data-model
[docs.event]: ../../../setup/getting-started/sending-your-first-event.md
[docs.guarantees#best-effort-delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.guarantees]: ../../../about/guarantees.md
[docs.monitoring#logs]: ../../../usage/administration/monitoring.md#logs
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[urls.aws_elasticsearch]: https://aws.amazon.com/elasticsearch-service/
[urls.aws_elasticsearch_regions]: https://docs.aws.amazon.com/general/latest/gr/rande.html#elasticsearch-service-regions
[urls.elasticsearch]: https://www.elastic.co/products/elasticsearch
[urls.elasticsearch_sink_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22sink%3A+elasticsearch%22+label%3A%22Type%3A+bug%22
[urls.elasticsearch_sink_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22sink%3A+elasticsearch%22+label%3A%22Type%3A+enhancement%22
[urls.elasticsearch_sink_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22sink%3A+elasticsearch%22
[urls.elasticsearch_sink_source]: https://github.com/timberio/vector/tree/master/src/sinks/elasticsearch.rs
[urls.new_elasticsearch_sink_bug]: https://github.com/timberio/vector/issues/new?labels=sink%3A+elasticsearch&labels=Type%3A+bug
[urls.new_elasticsearch_sink_enhancement]: https://github.com/timberio/vector/issues/new?labels=sink%3A+elasticsearch&labels=Type%3A+enhancement
[urls.new_elasticsearch_sink_issue]: https://github.com/timberio/vector/issues/new?labels=sink%3A+elasticsearch
[urls.strftime_specifiers]: https://docs.rs/chrono/0.3.1/chrono/format/strftime/index.html
[urls.vector_chat]: https://chat.vector.dev
