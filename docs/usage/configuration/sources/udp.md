---
description: Accept log events over UDP
---

# udp source

![](../../../.gitbook/assets/usp-source.svg)

The `udp` source allows you to ingest [`log`](../../../about/data-model.md#log) events over [UDP](https://en.wikipedia.org/wiki/User_Datagram_Protocol).

## Example

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[sources.<source-id>]
    # REQUIRED
    type    = "udp"
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
        <p>The network address to listen on, port included. Use <code>0.0.0.0:&lt;port&gt;</code> if
          you&apos;d like to accept data from remote instances, and <code>127.0.0.1:&lt;port&gt;</code> if
          you&apos;d like to accept data locally.</p>
        <p><code>example: 127.0.0.1:9000</code>
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
        <p>The name of the key to use for the host context. See <a href="udp.md#context">Context</a> for
          more info.</p>
        <p><code>default: &quot;host&quot;</code>
        </p>
      </td>
    </tr>
  </tbody>
</table>## Output

The `udp` source outputs [`log`](../../../about/data-model.md#log) events with the following [default schema](../../../about/data-model.md#default-schema):

{% code-tabs %}
{% code-tabs-item title="log" %}
```javascript
{
    "timestamp": "<timestamp>",
    "message": "<line>"
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## How It Works

### Context

Each line is augmented with the following context key:

* `"host"` - The address of the packet sender.

The key name can be changed with the [Context options](udp.md#options). An example can be seen in the [Output section](udp.md#output).

### Guarantees

Due to the nature of UDP, the `udp` source has a [**best effort delivery guarantee**](../../../about/guarantees.md#best-effort-delivery).

### Line Delimiters

Each line is read until a new line delimiter \(the `0xA` byte\) is found.

