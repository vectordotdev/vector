---
description: Discard events. Useful for testing and benchmarking.
---

# blackhole sink

![](../../../.gitbook/assets/blackhole-sink.svg)

The `blackhole` sink merely discards [`log`](../../../about/data-model.md#log) and [`metric`](../../../about/data-model.md#metric) events as they are received. This sink is useful for testing and [benchmarking](../../../comparisons/performance.md).

## Example

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[sinks.<sink-id>]
    # REQUIRED
    inputs       = ["{<source-id> | <transform-id>}", [ ... ]]
    type         = "blackhole"
    print_amount = 10000
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
      <td style="text-align:left"><code>print_amount</code>
      </td>
      <td style="text-align:center"><code>int</code>
      </td>
      <td style="text-align:left">
        <p>The number of events that must be received in order to print a summary
          of activity.</p>
        <p><code>example: 1000</code>
        </p>
      </td>
    </tr>
  </tbody>
</table>## Input

The `blackhole` sink accepts both [`log`](../../../about/data-model.md#log) and [`metrics`](../../../about/data-model.md#metric) events from a [source](../sources/) or [transform](../transforms/).

## Output

Vector will output summary lines every `print_amount` events received:

```text

```

## How It Works

The `blackhole` sink simply receives events and discards them, printing a summary to `STDOUT` every `print_amount` records.

## Resources

* [Source code](https://github.com/timberio/vector/blob/master/src/sinks/blackhole.rs)
* [Issues](https://github.com/timberio/vector/labels/Sink%3A%20Blackhole)

