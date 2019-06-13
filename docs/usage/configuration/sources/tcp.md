---
description: Accept log events over TCP
---

# tcp source

![](../../../.gitbook/assets/tcp-source.svg)

The `tcp` source allows you to ingest [`log`](../../../about/data-model.md#log) events over [TCP](https://en.wikipedia.org/wiki/Transmission_Control_Protocol).

## Example

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[sources.<source-id>]
    # REQUIRED
    type    = "tcp"
    address = "0.0.0.0:5000"
    
    # OPTIONAL - general
    max_length            = 2048
    shutdown_timeout_secs = 10
    
    # OPTIONAL - context
    host_key = "host"
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
      <td style="text-align:left"><code>address</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>The TCP address to listen on, port included. Use <code>0.0.0.0:&lt;port&gt;</code> if
          you&apos;d like to accept data from remote instances, and <code>127.0.0.1:&lt;port&gt;</code> if
          you&apos;d like to accept data locally.</p>
        <p><code>example: &quot;127.0.0.1:9000</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><b>OPTIONAL</b> - general</td>
      <td style="text-align:center"></td>
      <td style="text-align:left"></td>
    </tr>
    <tr>
      <td style="text-align:left"><code>max_length</code>
      </td>
      <td style="text-align:center"><code>int</code>
      </td>
      <td style="text-align:left">
        <p>The maximum length, in bytes, that a single message can be. If exceeded,
          the message will be discarded.</p>
        <p><code>default: 102400</code> (100 mib)</p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>shutdown_timeout_secs</code>
      </td>
      <td style="text-align:center"><code>int</code>
      </td>
      <td style="text-align:left">
        <p>The timeout, in seconds, before a connection is forcefully closed during
          shutdown.</p>
        <p><code>default: 30</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><b>OPTIONAL</b> - context</td>
      <td style="text-align:center"></td>
      <td style="text-align:left"></td>
    </tr>
    <tr>
      <td style="text-align:left"><code>host_key</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>The name of the key to use for the host context. See <a href="tcp.md#context">Context</a> for
          more info.</p>
        <p><code>default: &quot;host&quot;</code>
        </p>
      </td>
    </tr>
  </tbody>
</table>## Output

The `tcp` source outputs [`log`](../../../about/data-model.md#log) events with the following [default schema](../../../about/data-model.md#default-schema):

{% code-tabs %}
{% code-tabs-item title="log" %}
```javascript
{
    "timestamp": "<timestamp>",
    "message": "<line>",
    "host": "<host>"
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## How It Works

### Context

Each line is augmented with the following context key:

* `"host"` - The address of the connected upstream client.

The key names can be changed with the [Context options](tcp.md#options). An example can be seen in the [Output section](tcp.md#output).

### Guarantees

The `tcp` source is capable of achieving an [**at least once delivery guarantee**](../../../about/guarantees.md#at-least-once-delivery) if your [pipeline is configured to achieve this](../../../about/guarantees.md#at-least-once-delivery).

### Line Delimiters

Each line is read until a new line delimiter \(the `0xA` byte\) is found.

## Resources

* [Source code](https://github.com/timberio/vector/blob/master/src/sources/tcp.rs)
* [Issues](https://github.com/timberio/vector/labels/Source%3A%20TCP)

