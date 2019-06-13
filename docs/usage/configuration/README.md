---
description: How to configure Vector
---

# Configuration

![](../../.gitbook/assets/configure.svg)

This section will cover configuring Vector and creating [pipelines](../../about/concepts.md#pipelines) like the one shown above. The configuration process is designed to be simple, requiring only a _single_ [TOML](https://github.com/toml-lang/toml) file, which can be specified via the [`--config` flag](../administration/starting.md#options) when [starting](../administration/starting.md) vector:

```bash
vector --config /etc/vector/vector.toml
```

The meat of configuration is covered within the following sub-sections:

{% page-ref page="sources/" %}

{% page-ref page="transforms/" %}

{% page-ref page="sinks/" %}

## Example

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
# Define one or more sources.
[sources.nginx_logs]
    type         = "field"
    path         = "/var/log/nginx*.log"
    ignore_older = 86400 # 1 day

# Define transforms to parse, sample, and more.
[transforms.nginx_parser]
    inputs       = ["nginx_logs"]
    type         = "format_parser"
    format       = "nginx"

[transforms.nginx_sampler]
    inputs       = ["nginx_parser"]
    type         = "sampler"
    hash_field   = "request_id" # sample _entire_ requests
    rate         = 10 # only keep 10%

# Define one or more sinks for sending your data.
[sinks.es_cluster]
    inputs       = ["nginx_sampler"]
    type         = "elasticsearch"
    host         = "79.12.221.222:9200"

[sinks.s3_archives]
    inputs       = ["nginx_parser"] # don't sample
    type         = "s3"
    region       = "us-east-1"
    bucket       = "my_log_archives"
    buffer_size  = 10000000 # 10mb uncompressed
    gzip         = true
    encoding     = "ndjson"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

You can find more examples in the [examples section](examples/).

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
      <td style="text-align:left"><code>data_dir</code>
      </td>
      <td style="text-align:center"><code>string</code>
      </td>
      <td style="text-align:left">
        <p>The directory used for buffers and state management. See <a href="./#data-directory">Data Directory</a> below.</p>
        <p><code>no default</code>
        </p>
      </td>
    </tr>
    <tr>
      <td style="text-align:left"><code>sources</code>
      </td>
      <td style="text-align:center"><code>table</code>
      </td>
      <td style="text-align:left">A table of <a href="sources/">sources</a>.</td>
    </tr>
    <tr>
      <td style="text-align:left"><code>transforms</code>
      </td>
      <td style="text-align:center"><code>table</code>
      </td>
      <td style="text-align:left">A table of <a href="transforms/">transforms</a>.</td>
    </tr>
    <tr>
      <td style="text-align:left"><code>sinks</code>
      </td>
      <td style="text-align:center"><code>table</code>
      </td>
      <td style="text-align:left">A table of <a href="sinks/">sinks</a>.</td>
    </tr>
  </tbody>
</table>## How It Works

### Composition \(Pipelines\)

The primary purpose of the configuration file is to compose [pipelines](../../about/concepts.md#pipelines). Pipelines are form by connecting [sources](sources/), [transforms](transforms/), and [sinks](sinks/). You can learn more about creating pipelines with the the following guide:

{% page-ref page="../../setup/getting-started/creating-your-first-pipeline.md" %}

### Data Directory

Vector requires a `data_directory` for on-disk operations. Currently, the only operation using this directory are Vector's [on-disk buffers](sinks/buffer.md#on-disk). Buffers, by default, are [memory-based](sinks/buffer.md#in-memory), but if you switch them to disk-based you'll need to specify a `data_directory`.

### Environment Variables

Vector will interpolate environment variables within your configuration file with the following syntax:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```c
[transforms.add_host]
    type = "add_fields"
    
    [transforms.add_host.fields]
        host = "${HOSTNAME}"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

The entire `${HOSTNAME}` variable will be replaced, hence the requirement of quotes around the definition.

#### Escaping

You can escape environment variable by preceding them with a `$` character. For example `$${HOSTNAME}` will be treated _literally_ in the above environment variable example.

### Format

The Vector configuration file requires the [TOML](https://github.com/toml-lang/toml#table-of-contents) format for it simplicity, explicitness, and relaxed white-space parsing. This makes it ideal for use in configuration managers like [Confd](http://www.confd.io/) where white space is not always tightly controlled.

### Location

The location of your Vector configuration file depends on your [platform](../../setup/installation/platforms/) or [operating system](../../setup/installation/operating-systems/). For most Linux based systems the file can be found at `/etc/vector/vector.toml`.

### Value Types

All TOML values types are supported. For convenience this includes:

* [Strings](https://github.com/toml-lang/toml#string)
* [Integers](https://github.com/toml-lang/toml#integer)
* [Floats](https://github.com/toml-lang/toml#float)
* [Booleans](https://github.com/toml-lang/toml#boolean)
* [Offset Date-Times](https://github.com/toml-lang/toml#offset-date-time)
* [Local Date-Times](https://github.com/toml-lang/toml#local-date-time)
* [Local Dates](https://github.com/toml-lang/toml#local-date)
* [Local Times](https://github.com/toml-lang/toml#local-time)
* [Arrays](https://github.com/toml-lang/toml#array)
* [Tables](https://github.com/toml-lang/toml#table)

