---
description: Stream log events to a file
---

# file sink

## Example

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[sinks.<sink-id>]
    # REQUIRED
    inputs     = ["{<source-id> | <transform-id>}", [ ... ]]
    type       = "file"
    path       = "<path>"
    encoding   = "ndjson"
    
    # OPTIONAL - Generic
    compression   = "gzip"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

## Input

The `file` sink accepts only [`log`](../../../about/data-model.md#log) events from a [source](../sources/) or [transform](../transforms/).

## Output

The `file` sink streams events to a file. When flushed, Vector will produce a file encoded via the `encoding` [option](file.md#options), each encoding type is demonstrated below:

{% code-tabs %}
{% code-tabs-item title="ndjson" %}
```javascript
{"timestamp": 1557932537, "message": "GET /roi/evolve/embrace/transparent", "host": "Stracke8362", "process_id": 914, "remote_addr": "30.163.82.140", "response_code": 504, "bytes": 29763} 
{"timestamp": 1557933548, "message": "PUT /value-added/b2b", "host": "Wiza2458", "process_id": 775, "remote_addr": "30.163.82.140", "response_code": 503, "bytes": 9468}
{"timestamp": 1557933742, "message": "DELETE /reinvent/interfaces", "host": "Herman3087", "process_id": 775, "remote_addr": "43.246.221.247", "response_code": 503, "bytes": 9700}
```
{% endcode-tabs-item %}

{% code-tabs-item title="text" %}
```text
30.163.82.140 - Stracke8362 914 [2019-05-15T11:17:57-04:00] "GET /roi/evolve/embrace/transparent" 504 29763
190.218.92.219 - Wiza2458 775 [2019-05-15T11:17:57-04:00] "PUT /value-added/b2b" 503 9468
43.246.221.247 - Herman3087 294 [2019-05-15T11:17:57-04:00] "DELETE /reinvent/interfaces" 503 9700
```
{% endcode-tabs-item %}
{% endcode-tabs %}

The above examples are purposefully small for demonstration purposes. You can read more about encoding in the [Encoding](file.md#encoding) section.

## How It Works

### Encoding

The `file` sink encodes [events](../../../about/data-model.md#event) before writing them them. Each type is described in more detail below.

#### text

When encoding [events](../../../about/data-model.md#event) to `text` Vector will use the raw value of the `"message"` field and new line delimit \(the `0xA` byte\) the contents.

#### ndjson

When encoding [events](../../../about/data-model.md#event) to `ndjson`, Vector will encode the entire [event](../../../about/concepts.md#events) to JSON and new line delimit \(the `0xA` byte\) the contents.

#### nil \(default\)

If left unspecified, Vector will dynamically choose the appropriate encoding. If an [event](../../../about/concepts.md#events) is explicitly structured then it will be printed as `json`, if it is not, it will be printed as `text`. This provides the path of least surprise for different [pipelines](../../../about/concepts.md#pipelines).

For example, take the simple [`tcp` source](../sources/tcp.md) to `console` sink pipeline. The data coming from the `tcp` source is raw text lines, therefore, if you connected it directly to this sink you would expect to see those same raw text lines. Alternatively, if you parsed that data with a [transform](../transforms/), you would expect to see encoded structured data.

## Resources

* [Source code](https://github.com/timberio/vector/blob/master/src/sinks/elasticsearch.rs)
* [Issues](https://github.com/timberio/vector/labels/Sink%3A%20ES)

