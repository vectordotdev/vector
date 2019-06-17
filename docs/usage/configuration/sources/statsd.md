---
description: Accept metric events from a StatsD daemon
---

# statsd source

![](../../../.gitbook/assets/statsd-source.svg)

The `statsd` source allows you to ingest [`metric`](../../../about/data-model.md#metric) events from StatsD.

## Example

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[sources.<source-id>]
    # REQUIRED
    type    = "statsd"
    address = "<socket-address>"
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
        <p>StatsD UDP socket address</p>
        <p><code>example: &quot;127.0.0.1:8126&quot;</code>
        </p>
      </td>
    </tr>
  </tbody>
</table>## Output

The `statsd` source outputs [`metric`](../../../about/data-model.md#metric) event types with the following structure:

TODO: Add metric structure example below.

## How It Works

TODO: fill in

