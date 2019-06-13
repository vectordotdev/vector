---
description: Stream log events to a Kafka stream
---

# kafka sink

![](../../../.gitbook/assets/kafka-sink.svg)

The `kafka` sink streams [`log`](../../../about/data-model.md#log) events to a [Kafka stream](https://kafka.apache.org/).

## Example

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[sinks.<sink-id>]
    # REQUIRED
    inputs            = ["{<source-id> | <transform-id>}", [ ... ]]
    type              = "kafka"
    bootstrap_servers = "10.14.22.123:9092,10.14.23.332:9092"
    topic             = "<topic>"

    # OPTIONAL - generic
    key_field = "<key-field>"
    encoding  = "json"
    
    # OPTIONAL - buffer
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
      <td style="text-align:left"><code>bootstrap_servers</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>A <code>,</code> delimited list of hosts.</p>
        <p><code>example: &quot;10.14.22.123:9092,10.14.23.332:9092&quot;</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>topic</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>The topic name to write events to.</p>
        <p><code>example: &quot;my-topic&quot;</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><b>OPTIONAL</b> - generic</td>
      <td style="text-align:center"></td>
      <td style="text-align:left"></td>
    </tr>
    <tr>
      <td style="text-align:left"><code>key_field</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>The <a href="../../../about/data-model.md#event">event</a> field to use
          for the topic key. If unspecified, the key will be randomly generated.
          If the field does not exist on the event, a blank value will be used. See
          <a
          href="kafka.md#partitioning">Partitioning</a>for more info.</p>
        <p><code>example: &quot;partition_key&quot;</code>
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
          href="kafka.md#encoding">Encoding</a>below for more info.</p>
        <p><code>enum: &quot;text&quot;, &quot;json&quot;</code>
        </p>
      </td>
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

The `kafka` sink accepts only [`log`](../../../about/data-model.md#log) events from a [source](../sources/) or [transform](../transforms/).

## Output

The `kafka` sink streams events to Kafka encoded via the `encoding` [option](tcp.md#options). Each encoding is demonstrated below:

{% code-tabs %}
{% code-tabs-item title="json" %}
```http
{"timestamp": 1557932537, "message": "GET /roi/evolve/embrace/transparent", "host": "Stracke8362", "process_id": 914, "remote_addr": "30.163.82.140", "response_code": 504, "bytes": 29763} 
```
{% endcode-tabs-item %}

{% code-tabs-item title="text" %}
```http
30.163.82.140 - Stracke8362 914 [2019-05-15T11:17:57-04:00] "GET /roi/evolve/embrace/transparent" 504 29763
```
{% endcode-tabs-item %}
{% endcode-tabs %}

The above examples are purposefully small for demonstration purposes. You can read more about encoding in the [Encoding](tcp.md#encoding) section.

## How It Works

### Encoding

The `kafka` sink encodes [events](../../../about/data-model.md#event) before flushing them. Each encoding type is described in more detail below.

#### text

When encoding [events](../../../about/data-model.md#event) to `text` Vector will submit the `"message"` field only.

#### json

When encoding events to `json`, Vector will encode the entire [event](../../../about/concepts.md#events) to JSON.

#### nil \(default\)

If left unspecified, Vector will dynamically choose the appropriate encoding. If an [event](../../../about/concepts.md#events) is explicitly structured then it will be encoded as `json`, if it is not, it will be encoded as `text`. This provides the path of least surprise for different [pipelines](../../../about/concepts.md#pipelines).

For example, take the simple [`tcp` source](../sources/tcp.md) to `kafka` sink pipeline. The data coming from the `tcp` source is raw text lines, therefore, if you connected it directly to this sink you would expect to see those same raw text lines. Alternatively, if you parsed that data with a [transform](../transforms/), you would expect to see encoded structured data.

### Partitioning

In order to partition data within a Kafka topic, you must specify a `key_field`. This is the name of the field on your event to use as the value for the partition key. Partitioning data in Kafka is generally used to group and maintain order of data sharing the same partition key. You can [read more about Kafka partitioning in the Kafka docs](https://cwiki.apache.org/confluence/display/KAFKA/A+Guide+To+The+Kafka+Protocol#AGuideToTheKafkaProtocol-Partitioningandbootstrapping).

For example, if `key_field` is set to `partition_key` and an event has a `"partition_key"` field, then the value of that field will be used as the key when writing data to Kafka. If the event does not have a `"partition_key` field, then Vector will simply use a blank value.

### Retry Policy

Vector will retry failed requests \(status `== 429`, `>= 500`, and `!= 501`\). Other responses will not be retried. You can control the number of retry attempts and backoff rate with the `retry_attempts` and `retry_backoff_secs` options.

### Streaming

Data is streamed in a real-time, per-record basis. It is not batched.

## Resources

* [Source code](https://github.com/timberio/vector/blob/master/src/sinks/kafka.rs)
* [Issues](https://github.com/timberio/vector/labels/Sink%3A%20Kafka)

