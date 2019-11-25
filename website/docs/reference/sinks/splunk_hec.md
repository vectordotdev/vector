---
delivery_guarantee: "at_least_once"
event_types: ["log"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22sink%3A+splunk_hec%22
operating_systems: ["linux","macos","windows"]
sidebar_label: "splunk_hec|[\"log\"]"
source_url: https://github.com/timberio/vector/tree/master/src/sinks/splunk_hec.rs
status: "prod-ready"
title: "splunk_hec sink"
unsupported_operating_systems: []
---

The `splunk_hec` sink [batches](#buffers-and-batches) [`log`][docs.data-model#log] events to a [Splunk HTTP Event Collector][urls.splunk_hec].

## Configuration

import Tabs from '@theme/Tabs';

<Tabs
  block={true}
  defaultValue="common"
  values={[
    { label: 'Common', value: 'common', },
    { label: 'Advanced', value: 'advanced', },
  ]
}>

import TabItem from '@theme/TabItem';

<TabItem value="common">

import CodeHeader from '@site/src/components/CodeHeader';

<CodeHeader fileName="vector.toml" learnMoreUrl="/docs/setup/configuration"/ >

```toml
[sinks.my_sink_id]
  # REQUIRED - General
  type = "splunk_hec" # example, must be: "splunk_hec"
  inputs = ["my-source-id"] # example
  host = "my-splunk-host.com" # example
  token = "A94A8FE5CCB19BA61C4C08" # example
  
  # REQUIRED - requests
  encoding = "ndjson" # example, enum
```

</TabItem>
<TabItem value="advanced">

<CodeHeader fileName="vector.toml" learnMoreUrl="/docs/setup/configuration" />

```toml
[sinks.my_sink_id]
  # REQUIRED - General
  type = "splunk_hec" # example, must be: "splunk_hec"
  inputs = ["my-source-id"] # example
  host = "my-splunk-host.com" # example
  token = "A94A8FE5CCB19BA61C4C08" # example
  
  # REQUIRED - requests
  encoding = "ndjson" # example, enum
  
  # OPTIONAL - General
  healthcheck = true # default
  
  # OPTIONAL - Batching
  batch_size = 1049000 # default, bytes
  batch_timeout = 1 # default, seconds
  
  # OPTIONAL - Requests
  rate_limit_duration = 1 # default, seconds
  rate_limit_num = 10 # default
  request_in_flight_limit = 10 # default
  request_timeout_secs = 60 # default, seconds
  retry_attempts = 5 # default
  retry_backoff_secs = 5 # default, seconds
  
  # OPTIONAL - Buffer
  [sinks.my_sink_id.buffer]
    type = "memory" # default, enum
    max_size = 104900000 # example, no default, bytes, relevant when type = "disk"
    num_items = 500 # default, events, relevant when type = "memory"
    when_full = "block" # default, enum
  
  # OPTIONAL - Tls
  [sinks.my_sink_id.tls]
    ca_path = "/path/to/certificate_authority.crt" # example, no default
    crt_path = "/path/to/host_certificate.crt" # example, no default
    key_pass = "PassWord1" # example, no default
    key_path = "/path/to/host_certificate.key" # example, no default
    verify_certificate = true # default
    verify_hostname = true # default
```

</TabItem>

</Tabs>

## Options

import Fields from '@site/src/components/Fields';

import Field from '@site/src/components/Field';

<Fields filters={true}>


<Field
  common={false}
  defaultValue={1049000}
  enumValues={null}
  examples={[1049000]}
  name={"batch_size"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"int"}
  unit={"bytes"}
  >

### batch_size

The maximum size of a batch before it is flushed. See [Buffers & Batches](#buffers-batches) for more info.


</Field>


<Field
  common={false}
  defaultValue={1}
  enumValues={null}
  examples={[1]}
  name={"batch_timeout"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"int"}
  unit={"seconds"}
  >

### batch_timeout

The maximum age of a batch before it is flushed. See [Buffers & Batches](#buffers-batches) for more info.


</Field>


<Field
  common={false}
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"buffer"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"table"}
  unit={null}
  >

### buffer

Configures the sink specific buffer.

<Fields filters={false}>


<Field
  common={false}
  defaultValue={"memory"}
  enumValues={{"memory":"Stores the sink's buffer in memory. This is more performant (~3x), but less durable. Data will be lost if Vector is restarted abruptly.","disk":"Stores the sink's buffer on disk. This is less performance (~3x),  but durable. Data will not be lost between restarts."}}
  examples={["memory","disk"]}
  name={"type"}
  nullable={false}
  path={"buffer"}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"string"}
  unit={null}
  >

#### type

The buffer's type / location. `disk` buffers are persistent and will be retained between restarts.


</Field>


<Field
  common={false}
  defaultValue={"block"}
  enumValues={{"block":"Applies back pressure when the buffer is full. This prevents data loss, but will cause data to pile up on the edge.","drop_newest":"Drops new data as it's received. This data is lost. This should be used when performance is the highest priority."}}
  examples={["block","drop_newest"]}
  name={"when_full"}
  nullable={false}
  path={"buffer"}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"string"}
  unit={null}
  >

#### when_full

The behavior when the buffer becomes full.


</Field>


<Field
  common={false}
  defaultValue={null}
  enumValues={null}
  examples={[104900000]}
  name={"max_size"}
  nullable={true}
  path={"buffer"}
  relevantWhen={{"type":"disk"}}
  required={false}
  templateable={false}
  type={"int"}
  unit={"bytes"}
  >

#### max_size

The maximum size of the buffer on the disk.


</Field>


<Field
  common={false}
  defaultValue={500}
  enumValues={null}
  examples={[500]}
  name={"num_items"}
  nullable={true}
  path={"buffer"}
  relevantWhen={{"type":"memory"}}
  required={false}
  templateable={false}
  type={"int"}
  unit={"events"}
  >

#### num_items

The maximum number of [events][docs.data-model#event] allowed in the buffer.


</Field>


</Fields>

</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={{"ndjson":"Each event is encoded into JSON and the payload is new line delimited.","text":"Each event is encoded into text via the `message` key and the payload is new line delimited."}}
  examples={["ndjson","text"]}
  name={"encoding"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"string"}
  unit={null}
  >

### encoding

The encoding format used to serialize the events before outputting.


</Field>


<Field
  common={false}
  defaultValue={true}
  enumValues={null}
  examples={[true,false]}
  name={"healthcheck"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"bool"}
  unit={null}
  >

### healthcheck

Enables/disables the sink healthcheck upon start. See [Health Checks](#health-checks) for more info.


</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["my-splunk-host.com"]}
  name={"host"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"string"}
  unit={null}
  >

### host

Your Splunk HEC host. See [Setup](#setup) for more info.


</Field>


<Field
  common={false}
  defaultValue={1}
  enumValues={null}
  examples={[1]}
  name={"rate_limit_duration"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"int"}
  unit={"seconds"}
  >

### rate_limit_duration

The window used for the `request_rate_limit_num` option See [Rate Limits](#rate-limits) for more info.


</Field>


<Field
  common={false}
  defaultValue={10}
  enumValues={null}
  examples={[10]}
  name={"rate_limit_num"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"int"}
  unit={null}
  >

### rate_limit_num

The maximum number of requests allowed within the[`rate_limit_duration`](#rate_limit_duration) window. See [Rate Limits](#rate-limits) for more info.


</Field>


<Field
  common={false}
  defaultValue={10}
  enumValues={null}
  examples={[10]}
  name={"request_in_flight_limit"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"int"}
  unit={null}
  >

### request_in_flight_limit

The maximum number of in-flight requests allowed at any given time. See [Rate Limits](#rate-limits) for more info.


</Field>


<Field
  common={false}
  defaultValue={60}
  enumValues={null}
  examples={[60]}
  name={"request_timeout_secs"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"int"}
  unit={"seconds"}
  >

### request_timeout_secs

The maximum time a request can take before being aborted. It is highly recommended that you do not lower value below the service's internal timeout, as this could create orphaned requests, pile on retries, and result in deuplicate data downstream.


</Field>


<Field
  common={false}
  defaultValue={5}
  enumValues={null}
  examples={[5]}
  name={"retry_attempts"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"int"}
  unit={null}
  >

### retry_attempts

The maximum number of retries to make for failed requests. See [Retry Policy](#retry-policy) for more info.


</Field>


<Field
  common={false}
  defaultValue={5}
  enumValues={null}
  examples={[5]}
  name={"retry_backoff_secs"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"int"}
  unit={"seconds"}
  >

### retry_backoff_secs

The amount of time to wait before attempting a failed request again. See [Retry Policy](#retry-policy) for more info.


</Field>


<Field
  common={false}
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"tls"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"table"}
  unit={null}
  >

### tls

Configures the TLS options for connections from this sink.

<Fields filters={false}>


<Field
  common={false}
  defaultValue={null}
  enumValues={null}
  examples={["/path/to/certificate_authority.crt"]}
  name={"ca_path"}
  nullable={true}
  path={"tls"}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"string"}
  unit={null}
  >

#### ca_path

Absolute path to an additional CA certificate file, in DER or PEM format (X.509).


</Field>


<Field
  common={false}
  defaultValue={null}
  enumValues={null}
  examples={["/path/to/host_certificate.crt"]}
  name={"crt_path"}
  nullable={true}
  path={"tls"}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"string"}
  unit={null}
  >

#### crt_path

Absolute path to a certificate file used to identify this connection, in DER or PEM format (X.509) or PKCS#12. If this is set and is not a PKCS#12 archive,[`key_path`](#key_path) must also be set.


</Field>


<Field
  common={false}
  defaultValue={null}
  enumValues={null}
  examples={["/path/to/host_certificate.key"]}
  name={"key_path"}
  nullable={true}
  path={"tls"}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"string"}
  unit={null}
  >

#### key_path

Absolute path to a certificate key file used to identify this connection, in DER or PEM format (PKCS#8). If this is set,[`crt_path`](#crt_path) must also be set.


</Field>


<Field
  common={false}
  defaultValue={null}
  enumValues={null}
  examples={["PassWord1"]}
  name={"key_pass"}
  nullable={true}
  path={"tls"}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"string"}
  unit={null}
  >

#### key_pass

Pass phrase used to unlock the encrypted key file. This has no effect unless[`key_pass`](#key_pass) above is set.


</Field>


<Field
  common={false}
  defaultValue={true}
  enumValues={null}
  examples={[true,false]}
  name={"verify_certificate"}
  nullable={true}
  path={"tls"}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"bool"}
  unit={null}
  >

#### verify_certificate

If `true` (the default), Vector will validate the TLS certificate of the remote host. Do NOT set this to `false` unless you understand the risks of not verifying the remote certificate.


</Field>


<Field
  common={false}
  defaultValue={true}
  enumValues={null}
  examples={[true,false]}
  name={"verify_hostname"}
  nullable={true}
  path={"tls"}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"bool"}
  unit={null}
  >

#### verify_hostname

If `true` (the default), Vector will validate the configured remote host name against the remote host's TLS certificate. Do NOT set this to `false` unless you understand the risks of not verifying the remote hostname.


</Field>


</Fields>

</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["A94A8FE5CCB19BA61C4C08"]}
  name={"token"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"string"}
  unit={null}
  >

### token

Your Splunk HEC token. See [Setup](#setup) for more info.


</Field>


</Fields>

## Output

The `splunk_hec` sink [batches](#buffers-and-batches) [`log`][docs.data-model#log] events to a [Splunk HTTP Event Collector][urls.splunk_hec].
Batches are flushed via the [`batch_size`](#batch_size) or
[`batch_timeout`](#batch_timeout) options. You can learn more in the [buffers &
batches](#buffers--batches) section.

## How It Works

### Buffers & Batches

import SVG from 'react-inlinesvg';

<SVG src="/img/buffers-and-batches-serial.svg" />

The `splunk_hec` sink buffers & batches data as
shown in the diagram above. You'll notice that Vector treats these concepts
differently, instead of treating them as global concepts, Vector treats them
as sink specific concepts. This isolates sinks, ensuring services disruptions
are contained and [delivery guarantees][docs.guarantees] are honored.

*Batches* are flushed when 1 of 2 conditions are met:

1. The batch age meets or exceeds the configured[`batch_timeout`](#batch_timeout) (default: `1 seconds`).
2. The batch size meets or exceeds the configured[`batch_size`](#batch_size) (default: `1049000 bytes`).

*Buffers* are controlled via the [`buffer.*`](#buffer) options.

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
start.

#### Require Health Checks

If you'd like to exit immediately upon a health check failure, you can
pass the `--require-healthy` flag:

```bash
vector --config /etc/vector/vector.toml --require-healthy
```

#### Disable Health Checks

If you'd like to disable health checks for this sink you can set the[`healthcheck`](#healthcheck) option to `false`.

### Rate Limits

Vector offers a few levers to control the rate and volume of requests to the
downstream service. Start with the[`rate_limit_duration`](#rate_limit_duration) and[`rate_limit_num`](#rate_limit_num)
options to ensure Vector does not exceed the specified number of requests in
the specified window. You can further control the pace at which this window is
saturated with the[`request_in_flight_limit`](#request_in_flight_limit) option, which will guarantee no
more than the specified number of requests are in-flight at any given time.

Please note, Vector's defaults are carefully chosen and it should be rare that
you need to adjust these. If you found a good reason to do so please share it
with the Vector team by [opening an issie][urls.new_splunk_hec_sink_issue].

### Retry Policy

Vector will retry failed requests (status == `429`, >= `500`, and != `501`).
Other responses will _not_ be retried. You can control the number of retry
attempts and backoff rate with the[`retry_attempts`](#retry_attempts) and[`retry_backoff_secs`](#retry_backoff_secs) options.

### Setup

In order to supply values for both the[`host`](#host) and[`token`](#token) options you must first
setup a Splunk HTTP Event Collector. Please refer to the [Splunk setup
docs][urls.splunk_hec_setup] for a guide on how to do this. Once you've setup
your Spunk HTTP Collectory you'll be provided a[`host`](#host) and[`token`](#token) that you
should supply to the[`host`](#host) and[`token`](#token) options.


[docs.configuration#environment-variables]: /docs/setup/configuration#environment-variables
[docs.data-model#event]: /docs/about/data-model#event
[docs.data-model#log]: /docs/about/data-model#log
[docs.guarantees]: /docs/about/guarantees
[urls.new_splunk_hec_sink_issue]: https://github.com/timberio/vector/issues/new?labels=sink%3A+splunk_hec
[urls.splunk_hec]: http://dev.splunk.com/view/event-collector/SP-CAAAE6M
[urls.splunk_hec_setup]: https://docs.splunk.com/Documentation/Splunk/latest/Data/UsetheHTTPEventCollector
