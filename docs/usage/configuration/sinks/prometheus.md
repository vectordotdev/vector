---
description: Expose metrics data via a Prometheus scrapable endpoint
---

# prometheus sink

![](../../../assets/prometheus-sink.svg)

The `prometheus` sink exposes [`metric`](../../../about/data-model.md#metric) events via a Prometheus [scrapable endpoint](https://prometheus.io/docs/prometheus/latest/configuration/configuration/#scrape_config).

## Example

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[sinks.<sink-id>]
    # REQUIRED
    inputs = ["{<source-id> | <transform-id>}", [ ... ]]
    type   = "prometheus"

    # OPTIONAL - generic
    address = "0.0.0.0:9598"
    
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
      <td style="text-align:left"><b>OPTIONAL </b>- Generic</td>
      <td style="text-align:center"></td>
      <td style="text-align:left"></td>
    </tr>
    <tr>
      <td style="text-align:left"><code>address</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>The exporter address to expose Prometheus metrics for scraping.</p>
        <p><code>default: &quot;0.0.0.0:9598&quot;</code>
        </p>
      </td>
    </tr>
  </tbody>
</table>## Input

The `prometheus` sink accepts only [`metric`](../../../about/data-model.md#metric) events from a [source](../sources/) or [transform](../transforms/).

## Output

The `prometheus` sink exposes prometheus metrics data via the configured `address`. The exposed data follows [Prometheus' text exposition format](https://github.com/prometheus/docs/blob/master/content/docs/instrumenting/exposition_formats.md#text-format-example):

```text

```

## How It Works



## Resources

* [Source code](https://github.com/timberio/vector/blob/master/src/sinks/prometheus.rs)
* [Issues](https://github.com/timberio/vector/labels/Sink%3A%20Prometheus)



