---
title: Configuration
description: Configuring Vector
---

This section covers configuring Vector and creating
[pipelines][docs.configuration#composition] like the [example](#example) below.
Vector requires only a _single_ [TOML][urls.toml] configurable file, which you
can specify via the [`--config` flag][docs.process-management#flags] when
[starting][docs.process-management#starting] vector:

```bash
vector --config /etc/vector/vector.toml
```

## Example

import CodeHeader from '@site/src/components/CodeHeader';

<CodeHeader fileName="vector.toml" />

```toml
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
  regex        = '^(?P<host>[w.]+) - (?P<user>[w]+) (?P<bytes_in>[d]+) [(?P<timestamp>.*)] "(?P<method>[w]+) (?P<path>.*)" (?P<status>[d]+) (?P<bytes_out>[d]+)$'

# Sample the data to save on cost
[transforms.apache_sampler]
  inputs       = ["apache_parser"]
  type         = "sampler"
  hash_field   = "request_id"                  # sample _entire_ requests
  rate         = 50                            # only keep 50%

# Send structured data to a short-term storage
[sinks.es_cluster]
  inputs       = ["apache_sampler"]            # only take sampled data
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
  batch_size   = 10000000                      # 10mb uncompressed
  gzip         = true                          # compress final objects
  encoding     = "ndjson"                      # new line delimited JSON
```

## Global Options

import Fields from '@site/src/components/Fields';

import Field from '@site/src/components/Field';

<Fields filters={true}>


<Field
  common={false}
  defaultValue={null}
  enumValues={null}
  examples={["/var/lib/vector"]}
  name={"data_dir"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"string"}
  unit={null}
  >

### data_dir

The directory used for persisting Vector state, such as on-disk buffers, file checkpoints, and more. Please make sure the Vector project has write permissions to this dir. See [Data Directory](#data-directory) for more info.


</Field>


</Fields>

## Specification



## How It Works

### Composition

The primary purpose of the configuration file is to compose pipelines. Pipelines
are formed by connecting [sources][docs.sources], [transforms][docs.transforms],
and [sinks][docs.sinks] through the `inputs` option.

Notice in the above example each input references the `id` assigned to a
previous source or transform.

### Data Directory

Vector requires a[`data_dir`](#data_dir) value for on-disk operations. Currently, the only
operation using this directory are Vector's on-disk buffers. Buffers, by
default, are memory-based, but if you switch them to disk-based you'll need to
specify a[`data_dir`](#data_dir).

### Environment Variables

Vector will interpolate environment variables within your configuration file
with the following syntax:

<CodeHeader fileName="vector.toml" />

```toml
[transforms.add_host]
  type = "add_fields"
    
  [transforms.add_host.fields]
    host = "${HOSTNAME}"
    environment = "${ENV:-development}" # default value when not present
```

import Alert from '@site/src/components/Alert';

<Alert type="info">

Interpolation is done before parsing the configuration file. As such, the
entire `${ENV_VAR}` variable will be replaced, hence the requirement of
quotes around the definition.

</Alert>

#### Escaping

You can escape environment variable by preceding them with a `$` character. For
example `$${HOSTNAME}` will be treated _literally_ in the above environment
variable example.

### Example Location

The location of your Vector configuration file depends on your [installation
method][docs.installation]. For most Linux based systems the file can be
found at `/etc/vector/vector.toml`.

### Format

The Vector configuration file requires the [TOML][urls.toml] format for it's
simplicity, explicitness, and relaxed white-space parsing. For more information,
please refer to the excellent [TOML documentation][urls.toml].

### Template Syntax

Select configuration options support Vector's template syntax to produce
dynamic values derived from the event's data. There are 2 special syntaxes:

1. Strptime specifiers. Ex: `date=%Y/%m/%d`
2. Event fields. Ex: `{{ field_name }}`

Each are described in more detail below.

#### Strptime specifiers

For simplicity, Vector allows you to supply [strptime \
specifiers][urls.strptime_specifiers] directly as part of the value to produce
formatted timestamp values based off of the event's `timestamp` field.

For example, given the following [`log` event][docs.data-model.log]:

```rust
LogEvent {
    "timestamp": chrono::DateTime<2019-05-02T00:23:22Z>,
    "message": "message"
    "host": "my.host.com"
}
```

And the following configuration:

<CodeHeader fileName="vector.toml" />

```toml
[sinks.my_s3_sink_id]
  type = "aws_s3"
  key_prefix = "date=%Y-%m-%d"
```

Vector would produce the following value for the `key_prefix` field:

```
date=2019-05-02
```

This effectively enables time partitioning.

##### Event fields

In addition to formatting the `timestamp` field, Vector allows you to directly
access event fields with the `{{ <field-name> }}` syntax.

For example, given the following [`log` event][docs.data-model.log]:

```rust
LogEvent {
    "timestamp": chrono::DateTime<2019-05-02T00:23:22Z>,
    "message": "message"
    "application_id":  1
}
```

And the following configuration:

<CodeHeader fileName="vector.toml" />

```toml
[sinks.my_s3_sink_id]
  type = "aws_s3"
  key_prefix = "application_id={{ application_id }}/date=%Y-%m-%d"
```

Vector would produce the following value for the `key_prefix` field:

```
application_id=1/date=2019-05-02
```

This effectively enables application specific time partitioning.

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


[docs.configuration#composition]: /docs/setup/configuration#composition
[docs.data-model.log]: /docs/about/data-model/log
[docs.installation]: /docs/setup/installation
[docs.process-management#flags]: /docs/administration/process-management#flags
[docs.process-management#starting]: /docs/administration/process-management#starting
[docs.sinks]: /docs/components/sinks
[docs.sources]: /docs/components/sources
[docs.transforms]: /docs/components/transforms
[urls.strptime_specifiers]: https://docs.rs/chrono/0.3.1/chrono/format/strftime/index.html
[urls.toml]: https://github.com/toml-lang/toml
