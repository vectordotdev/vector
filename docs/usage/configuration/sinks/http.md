---
description: Batch and flush log events over HTTP
---

# http sink

![](../../../.gitbook/assets/http-sink%20%281%29.svg)

The `http` sink batches and flushes [`log`](../../../about/data-model.md#log) events to a generic [HTTP](https://en.wikipedia.org/wiki/Hypertext_Transfer_Protocol) endpoint.

## Example

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[sinks.<sink-id>]
    # REQUIRED
    inputs   = ["{<source-id> | <transform-id>}", [ ... ]]
    type     = "http"
    uri      = "https://my.service.com/path"
    encoding = "ndjson"

    # OPTIONAL - Generic
    compression     = "none"
    healthcheck_uri = "https://my.service.com/_health"
    
    # OPTIONAL - Batch
    batch_size    = 1048576 # 1mib
    batch_timeout = 5 # 5 seconds
    
    # OPTIONAL - Request
    request_in_flight_limit          = 10
    request_timeout_secs             = 10
    request_rate_limit_duration_secs = 1
    request_rate_limit_num           = 10
    request_retry_attempts           = 5
    request_retry_backoff_secs       = 5
    
    # OPTIONAL - Basic auth
    [sinks.<sink-id>.basic_auth]
        user     = "user"
        password = "password"
    
    # OPTIONAL - Headers
    [sinks.<sink-id>.headers]
        x-my-header-key = "my header value"
    
    # OPTIONAL - Buffer
    [sinks.<sink-id>.buffer]
        type     = "disk"
        max_size = 100000000 # 100mb
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

<table>
  <thead>
    <tr>
      <th style="text-align:left">Key</th>
      <th style="text-align:center">Type</th>
      <th style="text-align:left">Description</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td style="text-align:left"><b>REQUIRED</b>
      </td>
      <td style="text-align:center"></td>
      <td style="text-align:left"></td>
    </tr>
    <tr>
      <td style="text-align:left"><code>uri</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>The <em>full</em> URI to the service (protocol, host, and path included).
          Query strings are allowed.</p>
        <p><code>example: &quot;http://domain.com/path&quot;</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>encoding</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>The encoding format used to serialize the events before flushing. See
          <a
          href="http.md#encoding">Encoding</a>below for more info.</p>
        <p><code>enum: &quot;text&quot;, &quot;ndjson&quot;</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><b>OPTIONAL</b> - Generic</td>
      <td style="text-align:center"></td>
      <td style="text-align:left"></td>
    </tr>
    <tr>
      <td style="text-align:left"><code>compression</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>The compression strategy to use to compress the payload before sending.
          See <a href="http.md#compression">Compression</a> for more info.
          <br /><code>enum: &quot;none&quot;, &quot;gzip&quot;</code>
        </p>
        <p><code>default: &quot;none&quot;</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>healthcheck_uri</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>A full URI that returns a 200 if the service is healthy. See <a href="http.md#healthchecks">Healtchecks</a> for
          more info.</p>
        <p><code>no default</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><b>OPTIONAL</b> - Batch</td>
      <td style="text-align:center"></td>
      <td style="text-align:left"></td>
    </tr>
    <tr>
      <td style="text-align:left"><code>batch_size</code>
      </td>
      <td style="text-align:center"><code>int</code>
      </td>
      <td style="text-align:left">
        <p>The maximum size of the <a href="./#batches">batch</a>, in bytes, before
          it is flushed. See <a href="http.md#batching">Batching</a> below.</p>
        <p><code>default: 1048576</code> (1mib)</p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>batch_timeout</code>
      </td>
      <td style="text-align:center"><code>int</code>
      </td>
      <td style="text-align:left">
        <p>The maximum age of a <a href="./#batches">batch</a>, in seconds, before
          it is flushed. See <a href="http.md#batching">Batching</a> below.</p>
        <p><code>default: 5</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><b>OPTIONAL</b> - Request</td>
      <td style="text-align:center"></td>
      <td style="text-align:left"></td>
    </tr>
    <tr>
      <td style="text-align:left"><code>request_in_flight_limit</code>
      </td>
      <td style="text-align:center"><code>int</code>
      </td>
      <td style="text-align:left">
        <p>The maximum number of in-flight requests allowed at any given time. See
          <a
          href="http.md#rate-limiting">Rate Limiting</a>for more info.</p>
        <p><code>default: 10</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>request_timeout_secs</code>
      </td>
      <td style="text-align:center"><code>int</code>
      </td>
      <td style="text-align:left">
        <p>The maximum time a request can take before being aborted. See <a href="http.md#timeouts">Timeouts</a> for
          more info.</p>
        <p><code>default: 10</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>rate_limit_duration</code>
      </td>
      <td style="text-align:center"><code>int</code>
      </td>
      <td style="text-align:left">
        <p>The window, in seconds, used for the <code>request_rate_limit_num</code> option.
          See <a href="http.md#rate-limiting">Rate Limiting</a> for more info.</p>
        <p><code>default: 1</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>rate_limit_num</code>
      </td>
      <td style="text-align:center"><code>int</code>
      </td>
      <td style="text-align:left">
        <p>The maximum number of requests allowed within the <code>rate_limit_duration</code> window.
          See <a href="http.md#rate-limiting">Rate Limiting</a> for more info.</p>
        <p><code>default: 10</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>retry_attempts</code>
      </td>
      <td style="text-align:center"><code>int</code>
      </td>
      <td style="text-align:left">
        <p>The maximum number of retries to make for failed requests. See <a href="http.md#retry-policy">Retry Policy</a> for
          more info.</p>
        <p><code>default: 5</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>retry_backoff_secs</code>
      </td>
      <td style="text-align:center"><code>int</code>
      </td>
      <td style="text-align:left">
        <p>The amount of time, in seconds, to wait before attempting a failed request
          again. See <a href="http.md#retry-policy">Retry Policy</a> for more info.</p>
        <p><code>default: 1</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><b>OPTIONAL</b> - Basic auth</td>
      <td style="text-align:center"><code>table</code>
      </td>
      <td style="text-align:left"></td>
    </tr>
    <tr>
      <td style="text-align:left"><code>basic_auth.user</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>The basic authentication user name. See <a href="http.md#headers">Basic Auth</a> for
          more info.</p>
        <p><code>example: &quot;user&quot;</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>basic_auth.password</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>The basic authentication password. See <a href="http.md#basic-auth">Basic Auth</a> for
          more info.</p>
        <p><code>example: &quot;pass&quot;</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><b>OPTIONAL</b> - Headers</td>
      <td style="text-align:center"><code>table</code>
      </td>
      <td style="text-align:left"></td>
    </tr>
    <tr>
      <td style="text-align:left"><code>headers.*</code>
      </td>
      <td style="text-align:center"><code>any</code>
      </td>
      <td style="text-align:left">The <code>headers</code> table accepts key value pairs that will add the
        headers accordingly. See <a href="http.md#headers">Headers</a> for more info.</td>
    </tr>
    <tr>
      <td style="text-align:left"><b>OPTIONAL</b> - Buffer</td>
      <td style="text-align:center"></td>
      <td style="text-align:left"></td>
    </tr>
    <tr>
      <td style="text-align:left">&lt;code&gt;&lt;/code&gt;<a href="buffer.md"><code>buffer.*</code></a>&lt;code&gt;&lt;/code&gt;</td>
      <td
      style="text-align:center"><code>table</code>
        </td>
        <td style="text-align:left">A table that configures the sink specific buffer. See the <a href="buffer.md">*.buffer document</a>.</td>
    </tr>
  </tbody>
</table>## Input

The `http` sink accepts only [`log`](../../../about/data-model.md#log) events from a [source](../sources/) or [transform](../transforms/).

## Output

The `http` sink batches events up to the `batch_size` or `batch_timeout` [options](http.md#options). When flushed, Vector will produce an HTTP request to the configured endpoint. The encoding is dictated by the `encoding` option \(see [Encoding](http.md#encoding)\), each encoding is demonstrated below:

{% code-tabs %}
{% code-tabs-item title="ndjson" %}
```http
POST <uri> HTTP/1.1
Host: <host>
Content-Type: application/x-ndjson
Content-Length: 711

{"timestamp": 1557932537, "message": "GET /roi/evolve/embrace/transparent", "host": "Stracke8362", "process_id": 914, "remote_addr": "30.163.82.140", "response_code": 504, "bytes": 29763} 
{"timestamp": 1557933548, "message": "PUT /value-added/b2b", "host": "Wiza2458", "process_id": 775, "remote_addr": "30.163.82.140", "response_code": 503, "bytes": 9468}
{"timestamp": 1557933742, "message": "DELETE /reinvent/interfaces", "host": "Herman3087", "process_id": 775, "remote_addr": "43.246.221.247", "response_code": 503, "bytes": 9700}
```
{% endcode-tabs-item %}

{% code-tabs-item title="text" %}
```http
POST <uri> HTTP/1.1
Host: <host>
Content-Type: text/plain
Content-Length: 645

30.163.82.140 - Stracke8362 914 [2019-05-15T11:17:57-04:00] "GET /roi/evolve/embrace/transparent" 504 29763
190.218.92.219 - Wiza2458 775 [2019-05-15T11:17:57-04:00] "PUT /value-added/b2b" 503 9468
43.246.221.247 - Herman3087 294 [2019-05-15T11:17:57-04:00] "DELETE /reinvent/interfaces" 503 9700
```
{% endcode-tabs-item %}
{% endcode-tabs %}

The above examples are purposefully small for demonstration purposes. You can read more about encoding in the [Encoding](http.md#encoding) section.

## How It Works

### Basic Auth

The `basic_auth` table provides a convenient way to set the `Authorization` header according to [RFC 2617](https://tools.ietf.org/html/rfc2617). The `basic_auth` options take precedence over any `headers` options.

### Batching

The batch size and timeout is highly dependent on your downstream service. We recommend increasing the `batch_size` to the maximum your downstream service will support and decreasing the `batch_timeout` to a value that will not disrupt service or substantially increase cost. The lower the `batch_timeout` the more real time your data will be.

Please note, if you enable [compression](http.md#compression), the `batch_size` is based on the post-compressed size.

### Compression

Currently, Vector only supports the [`gzip` compression](https://en.wikipedia.org/wiki/Gzip) option. The entire request body will be encoded with Gzip and the `Content-Encoding` header will be set to `gzip`.

### Encoding

The `http` sink encodes [events](../../../about/data-model.md#event) before flushing them to the configured endpoint. Because the entire HTTP request is encoded, Vector can encode the request in different formats via the `encoding` option. Each encoding type is described in more detail below.

#### text

When encoding [events](../../../about/data-model.md#event) to `text` Vector will use the raw value of the `"message"` field and new line delimit \(the `0xA` byte\) the contents.

#### ndjson

When encoding events to `ndjson`, Vector will encode the object as [ndjson](http://ndjson.org/), which means the entire [event](../../../about/concepts.md#events) is JSON encoded and then new line \(the `0xA` byte\) delimited.

### Headers

Vector does not add any headers by default. You can set headers via the `headers` table as described in the [Options](http.md#example) section.

### Healthchecks

A `healthcheck_uri` is optional but should be supplied if possible. This endpoint must respond in under 10 seconds and return a `2XX` response code to be deemed "healthy". If the service does not return a `2XX`, Vector will mark the sink as unhealthy and will not

TODO: link to a healthchecks section?

### Rate Limiting

Vector offers a few levers to control the rate and volume of requests made to the downstream service. We recommend starting with the `rate_limit_duration` and `rate_limit_num` options to ensure Vector does not exceed the specified number of requests in the specified window. You can further control the pace at which this window is saturated with the `request_in_flight_limit` option, which will guarantee no more than the specified number of requests are in-flight at any given time.

### Retry Policy

Vector will retry failed requests \(status `== 429`, `>= 500`, and `!= 501`\). Other responses will not be retried. You can control the number of retry attempts and backoff rate with the `retry_attempts` and `retry_backoff_secs` options.

### Timeouts

The default `request_timeout_secs` is based on a general industry default of 30 seconds. It highly recommended that you do not lower this, unless you have also lowered the downstream service's timeout. This prevents orphaned requests, and ensures Vector does not pile on retries.

## Resources

* [Source code](https://github.com/timberio/vector/blob/master/src/sinks/http.rs)
* [Issues](https://github.com/timberio/vector/labels/Sink%3A%20HTTP)

