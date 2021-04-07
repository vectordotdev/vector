---
title: Configuration
description: Configuring Vector
---

This section covers configuring Vector and creating pipelines like the
[example below](#example). Vector's configuration uses the [TOML][urls.toml]
syntax, and the configuration file must be passed via the
[`--config` flag][docs.process-management#flags] when
[starting][docs.process-management#starting] Vector:

```bash
vector --config /etc/vector/vector.toml
```

## Example

```toml title="vector.toml"
# Set global options
data_dir = "/var/lib/vector"

# Ingest data by tailing one or more files
[sources.apache_logs]
  type         = "file"
  include      = ["/var/log/apache2/*.log"]    # supports globbing
  ignore_older = 86400                         # 1 day

# Structure and parse the data
[transforms.apache_parser]
  inputs       = ["apache_logs"]
  type         = "regex_parser"                # fast/powerful regex
  patterns      = ['^(?P<host>[w.]+) - (?P<user>[w]+) (?P<bytes_in>[d]+) [(?P<timestamp>.*)] "(?P<method>[w]+) (?P<path>.*)" (?P<status>[d]+) (?P<bytes_out>[d]+)$']

# Sample the data to save on cost
[transforms.apache_sample]
  inputs       = ["apache_parser"]
  type         = "sample"
  rate         = 50                            # only keep 50%

# Send structured data to a short-term storage
[sinks.es_cluster]
  inputs       = ["apache_sample"]            # only take sampled data
  type         = "elasticsearch"
  host         = "http://79.12.221.222:9200"   # local or external host
  index        = "vector-%Y-%m-%d"             # daily indices

# Send structured data to a cost-effective long-term storage
[sinks.s3_archives]
  inputs       = ["apache_parser"]             # don't sample for S3
  type         = "aws_s3"
  region       = "us-east-1"
  bucket       = "my-log-archives"
  key_prefix   = "date=%Y-%m-%d"               # daily partitions, hive friendly format
  compression  = "gzip"                        # compress final objects
  encoding     = "ndjson"                      # new line delimited JSON
  [sinks.s3_archives.batch]
    max_size   = 10000000                      # 10mb uncompressed

```

The key thing to note above is the use of the `inputs` option. This connects
Vector's component to create a pipeline. For a simple introduction, please
refer to the:

<Jump to="/guides/getting-started/your-first-pipeline/">Getting Started Guide</Jump>

## Reference

Vector provides a [full reference][docs.reference] that you can use to build
your configuration files.

<Jump to="/docs/reference/sources/">Sources</Jump>
<Jump to="/docs/reference/transforms/">Transforms</Jump>
<Jump to="/docs/reference/sinks/">Sinks</Jump>

And for more advanced techniques:

<Jump to="/docs/reference/env-vars/">Env Vars</Jump>
<Jump to="/docs/reference/global-options/">Global options</Jump>
<Jump to="/docs/reference/templates/">Template syntax</Jump>
<Jump to="/docs/reference/tests/">Tests</Jump>

## How It Works

### Configuration File Location

The location of your Vector configuration file depends on your [installation
method][docs.installation]. For most Linux based systems, the file can be
found at `/etc/vector/vector.toml`.

### Environment Variables

Vector will interpolate environment variables within your configuration file
with the following syntax:

```toml title="vector.toml"
[transforms.add_host]
  type = "add_fields"

  [transforms.add_host.fields]
    host = "${HOSTNAME}"
    environment = "${ENV:-development}" # default value when not present
```

<Alert type="info">

Interpolation is done before parsing the configuration file. As such, the
entire `${ENV_VAR}` variable will be replaced, hence the requirement of
quotes around the definition.

</Alert>

Please refer to the [environment variables reference][docs.reference.env-vars]
for more info.

### Multiple Configuration Files

You can pass multiple configuration files when starting Vector:

```bash
vector --config vector1.toml --config vector2.toml
```

Or use a [globbing syntax][urls.globbing]:

```bash
vector --config /etc/vector/*.toml
```

### Syntax

The Vector configuration file follows the [TOML][urls.toml] syntax for its
simplicity, explicitness, and relaxed white-space parsing. For more information,
please refer to the [TOML documentation][urls.toml].

### Template Syntax

Select configuration options support Vector's
[template syntax][docs.reference.templates] to produce dynamic values derived
from the event's data. Two syntaxes are supported for fields that support field
interpolation:

1. [Strptime specifiers][urls.strptime_specifiers]. Ex: `date=%Y/%m/%d`
2. [Log fields][docs.data-model.log]. Ex: `{{ field_name }}`
3. [Metric name, namespace, or tags][docs.data-model.metric]. Ex: `{{ name }} {{ namespace }} {{ tags.tag_name }}`

For example:

```toml title="vector.toml"
[sinks.es_cluster]
  type  = "elasticsearch"
  index = "user-{{ user_id }}-%Y-%m-%d"
```

The above `index` value will be calculated for _each_ event. For example, given
the following event:

```json
{
  "timestamp": "2019-05-02T00:23:22Z",
  "message": "message",
  "user_id": 2
}
```

The `index` value will result in:

```toml
index = "user-2-2019-05-02"
```

Learn more in the [template reference][docs.reference.templates].

### Types

All TOML values types are supported. For convenience this includes:

- [Strings](https://github.com/toml-lang/toml#string)
- [Integers](https://github.com/toml-lang/toml#integer)
- [Floats](https://github.com/toml-lang/toml#float)
- [Booleans](https://github.com/toml-lang/toml#boolean)
- [Offset Date-Times](https://github.com/toml-lang/toml#offset-date-time)
- [Local Date-Times](https://github.com/toml-lang/toml#local-date-time)
- [Local Dates](https://github.com/toml-lang/toml#local-date)
- [Local Times](https://github.com/toml-lang/toml#local-time)
- [Arrays](https://github.com/toml-lang/toml#array)
- [Tables](https://github.com/toml-lang/toml#table)

[docs.data-model]: /docs/about/data-model/
[docs.installation]: /docs/setup/installation/
[docs.process-management#flags]: /docs/administration/process-management/#flags
[docs.process-management#starting]: /docs/administration/process-management/#starting
[docs.reference.env-vars]: /docs/reference/env-vars/
[docs.reference.templates]: /docs/reference/templates/
[docs.reference]: /docs/reference/
[urls.globbing]: https://en.wikipedia.org/wiki/Glob_(programming)
[urls.strptime_specifiers]: https://docs.rs/chrono/0.4.11/chrono/format/strftime/index.html#specifiers
[urls.toml]: https://github.com/toml-lang/toml
