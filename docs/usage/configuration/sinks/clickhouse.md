---
title: "clickhouse sink" 
sidebar_label: "clickhouse"
---

The `clickhouse` sink [batches](#buffers-and-batches) [`log`][docs.data-model.log] events to [Clickhouse][urls.clickhouse] via the [`HTTP` Interface][urls.clickhouse_http].

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
  # REQUIRED - General
  type = "clickhouse" # enum
  inputs = ["my-source-id"]
  host = "http://localhost:8123"
  table = "mytable"
  
  # OPTIONAL - requests
  compression = "gzip" # default, enum
```

</TabItem>
<TabItem value="advanced">

```coffeescript
[sinks.my_sink_id]
  # REQUIRED - General
  type = "clickhouse" # enum
  inputs = ["my-source-id"]
  host = "http://localhost:8123"
  table = "mytable"
  
  # OPTIONAL - General
  database = "mydatabase" # no default
  healthcheck = true # default
  
  # OPTIONAL - Batching
  batch_size = 1049000 # default, bytes
  batch_timeout = 1 # default, seconds
  
  # OPTIONAL - Requests
  rate_limit_duration = 1 # default, seconds
  rate_limit_num = 5 # default
  request_in_flight_limit = 5 # default
  request_timeout_secs = 30 # default, seconds
  retry_attempts = 9223372036854775807 # default
  retry_backoff_secs = 9223372036854775807 # default, seconds
  
  # OPTIONAL - requests
  compression = "gzip" # default, enum
  
  # OPTIONAL - Basic auth
  [sinks.my_sink_id.basic_auth]
    password = "password"
    user = "username"
  
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
  defaultValue={1049000}
  enumValues={null}
  examples={[1049000]}
  name={"batch_size"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={"bytes"}>

### batch_size

The maximum size of a batch before it is flushed.


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

The maximum age of a batch before it is flushed.


</Option>


<Option
  defaultValue={"gzip"}
  enumValues={{"gzip":"The payload will be compressed in [Gzip][urls.gzip] format before being sent."}}
  examples={["gzip"]}
  name={"compression"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={true}
  type={"string"}
  unit={null}>

### compression

The compression strategy used to compress the encoded event data before outputting.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={["mydatabase"]}
  name={"database"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

### database

The database that contains the stable that data will be inserted into.


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
  examples={["http://localhost:8123"]}
  name={"host"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  simple={true}
  type={"string"}
  unit={null}>

### host

The host url of the [Clickhouse][urls.clickhouse] server.


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
  defaultValue={30}
  enumValues={null}
  examples={[30]}
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
  defaultValue={9223372036854775807}
  enumValues={null}
  examples={[9223372036854775807]}
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
  defaultValue={9223372036854775807}
  enumValues={null}
  examples={[9223372036854775807]}
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
  examples={["mytable"]}
  name={"table"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  simple={true}
  type={"string"}
  unit={null}>

### table

The table that data will be inserted into.


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

## How It Works

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

### Rate Limits

Vector offers a few levers to control the rate and volume of requests to the
downstream service. Start with the `rate_limit_duration` and `rate_limit_num`
options to ensure Vector does not exceed the specified number of requests in
the specified window. You can further control the pace at which this window is
saturated with the `request_in_flight_limit` option, which will guarantee no
more than the specified number of requests are in-flight at any given time.

Please note, Vector's defaults are carefully chosen and it should be rare that
you need to adjust these. If you found a good reason to do so please share it
with the Vector team by [opening an issie][urls.new_clickhouse_sink_issue].

### Retry Policy

Vector will retry failed requests (status == `429`, >= `500`, and != `501`).
Other responses will _not_ be retried. You can control the number of retry
attempts and backoff rate with the `retry_attempts` and `retry_backoff_secs` options.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring#logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `clickhouse_sink` issues][urls.clickhouse_sink_issues].
2. If encountered a bug, please [file a bug report][urls.new_clickhouse_sink_bug].
3. If encountered a missing feature, please [file a feature request][urls.new_clickhouse_sink_enhancement].
4. If you need help, [join our chat/forum community][urls.vector_chat]. You can post a question and search previous questions.

## Resources

* [**Issues**][urls.clickhouse_sink_issues] - [enhancements][urls.clickhouse_sink_enhancements] - [bugs][urls.clickhouse_sink_bugs]
* [**Source code**][urls.clickhouse_sink_source]


[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.data-model.log]: ../../../about/data-model/log.md
[docs.guarantees#best-effort-delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.monitoring#logs]: ../../../usage/administration/monitoring.md#logs
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[urls.clickhouse]: https://clickhouse.yandex/
[urls.clickhouse_http]: https://clickhouse.yandex/docs/en/interfaces/http/
[urls.clickhouse_sink_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22sink%3A+clickhouse%22+label%3A%22Type%3A+bug%22
[urls.clickhouse_sink_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22sink%3A+clickhouse%22+label%3A%22Type%3A+enhancement%22
[urls.clickhouse_sink_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22sink%3A+clickhouse%22
[urls.clickhouse_sink_source]: https://github.com/timberio/vector/tree/master/src/sinks/clickhouse.rs
[urls.gzip]: https://www.gzip.org/
[urls.new_clickhouse_sink_bug]: https://github.com/timberio/vector/issues/new?labels=sink%3A+clickhouse&labels=Type%3A+bug
[urls.new_clickhouse_sink_enhancement]: https://github.com/timberio/vector/issues/new?labels=sink%3A+clickhouse&labels=Type%3A+enhancement
[urls.new_clickhouse_sink_issue]: https://github.com/timberio/vector/issues/new?labels=sink%3A+clickhouse
[urls.vector_chat]: https://chat.vector.dev
