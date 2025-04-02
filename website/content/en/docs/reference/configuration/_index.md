---
title: Configuring Vector
short: Configuration
weight: 2
aliases: ["/docs/configuration", "/docs/setup/configuration"]
---

Vector is configured using a configuration file. This section contains a
comprehensive reference of all Vector configuration options.

## Example

The following is an example of a popular Vector configuration that ingests logs
from a file and routes them to both Elasticsearch and AWS S3. Your configuration
will differ based on your needs.

{{< tabs default="vector.yaml" >}}
{{< tab title="vector.yaml" >}}

```yaml
# Set global options
data_dir: "/var/lib/vector"

# Vector's API (disabled by default)
# Enable and try it out with the `vector top` command
# NOTE: this is _enabled_ for helm chart deployments, see: https://github.com/vectordotdev/helm-charts/blob/develop/charts/vector/examples/datadog-values.yaml#L78-L81
api:
  enabled: false
# address = "127.0.0.1:8686"

# Ingest data by tailing one or more files
sources:
  apache_logs:
    type: "file"
    include:
      - "/var/log/apache2/*.log" # supports globbing
    ignore_older_secs: 86400     # 1 day

# Structure and parse via Vector's Remap Language
transforms:
  apache_parser:
    inputs:
      - "apache_logs"
    type: "remap"
    source: ". = parse_apache_log(.message)"

  # Sample the data to save on cost
  apache_sampler:
    inputs:
      - "apache_parser"
    type: "sample"
    rate: 2 # only keep 50% (1/`rate`)

# Send structured data to a short-term storage
sinks:
  es_cluster:
    inputs:
      - "apache_sampler"       # only take sampled data
    type: "elasticsearch"
    endpoints:
      - "http://79.12.221.222:9200"
    bulk:
      index: "vector-%Y-%m-%d" # daily indices

  # Send structured data to a cost-effective long-term storage
  s3_archives:
    inputs:
      - "apache_parser" # don't sample for S3
    type: "aws_s3"
    region: "us-east-1"
    bucket: "my-log-archives"
    key_prefix: "date=%Y-%m-%d"   # daily partitions, hive friendly format
    compression: "gzip"           # compress final objects
    framing:
      method: "newline_delimited" # new line delimited...
    encoding:
      codec: "json"               # ...JSON
    batch:
      max_bytes: 10000000         # 10mb uncompressed
```

{{< /tab >}}
{{< tab title="vector.toml" >}}

```toml
# Set global options
data_dir = "/var/lib/vector"

# Vector's API (disabled by default)
# Enable and try it out with the `vector top` command
[api]
enabled = false
# address = "127.0.0.1:8686"

# Ingest data by tailing one or more files
[sources.apache_logs]
type              = "file"
include           = ["/var/log/apache2/*.log"]    # supports globbing
ignore_older_secs = 86400                         # 1 day

# Structure and parse via Vector's Remap Language
[transforms.apache_parser]
inputs = ["apache_logs"]
type   = "remap"
source = '''
. = parse_apache_log(.message)
'''

# Sample the data to save on cost
[transforms.apache_sampler]
inputs = ["apache_parser"]
type   = "sample"
rate   = 2                    # only keep 50% (1/`rate`)

# Send structured data to a short-term storage
[sinks.es_cluster]
inputs     = ["apache_sampler"]             # only take sampled data
type       = "elasticsearch"
endpoints  = ["http://79.12.221.222:9200"]  # local or external host
bulk.index = "vector-%Y-%m-%d"              # daily indices

# Send structured data to a cost-effective long-term storage
[sinks.s3_archives]
inputs          = ["apache_parser"]    # don't sample for S3
type            = "aws_s3"
region          = "us-east-1"
bucket          = "my-log-archives"
key_prefix      = "date=%Y-%m-%d"      # daily partitions, hive friendly format
compression     = "gzip"               # compress final objects
framing.method  = "newline_delimited"  # new line delimited...
encoding.codec  = "json"               # ...JSON
batch.max_bytes = 10000000             # 10mb uncompressed
```

{{< /tab >}}
{{< tab title="vector.json" >}}

```json
{
  "data_dir": "/var/lib/vector",
  "sources": {
    "apache_logs": {
      "type": "file",
      "include": [
        "/var/log/apache2/*.log"
      ],
      "ignore_older": 86400
    }
  },
  "transforms": {
    "apache_parser": {
      "inputs": [
        "apache_logs"
      ],
      "type": "remap",
      "source": ". = parse_apache_log(.message)"
    },
    "apache_sampler": {
      "inputs": [
        "apache_parser"
      ],
      "type": "sample",
      "rate": 50
    }
  },
  "sinks": {
    "es_cluster": {
      "inputs": [
        "apache_sampler"
      ],
      "type": "elasticsearch",
      "endpoints": ["http://79.12.221.222:9200"],
      "bulk": {
        "index": "vector-%Y-%m-%d"
      }
    },
    "s3_archives": {
      "inputs": [
        "apache_parser"
      ],
      "type": "aws_s3",
      "region": "us-east-1",
      "bucket": "my-log-archives",
      "key_prefix": "date=%Y-%m-%d",
      "compression": "gzip",
      "framing": {
        "method": "newline_delimited"
      },
      "encoding": {
        "codec": "json"
      },
      "batch": {
        "max_bytes": 10000000
      }
    }
  }
}
```

{{< /tab >}}
{{< /tabs >}}

To use this configuration file, specify it with the `--config` flag when
starting Vector:

{{< tabs default="YAML" >}}
{{< tab title="YAML" >}}

```shell
vector --config /etc/vector/vector.yaml
```

{{< /tab >}}
{{< tab title="TOML" >}}

```shell
vector --config /etc/vector/vector.toml
```

{{< /tab >}}
{{< tab title="JSON" >}}

```shell
vector --config /etc/vector/vector.json
```

{{< /tab >}}
{{< /tabs >}}

## Reference

### Components

{{< jump "/docs/reference/configuration/sources" >}} {{< jump
"/docs/reference/configuration/transforms" >}} {{< jump
"/docs/reference/configuration/sinks" >}}

### Advanced

{{< jump "/docs/reference/configuration/global-options" >}} {{< jump
"/docs/reference/configuration/template-syntax" >}}

## How it works

### Environment variables

Vector interpolates environment variables within your configuration file with
the following syntax:

```yaml
transforms:
  add_host:
    type: "remap"
    source: |
      # Basic usage. "$HOSTNAME" also works.
      .host = "${HOSTNAME}" # or "$HOSTNAME"

      # Setting a default value when not present.
      .environment = "${ENV:-development}"

      # Requiring an environment variable to be present.
      .tenant = "${TENANT:?tenant must be supplied}"
```

#### Default values

Default values can be supplied using `:-` or `-` syntax:

```yaml
option: "${ENV_VAR:-default}" # default value if variable is unset or empty
option: "${ENV_VAR-default}" # default value only if variable is unset
```

#### Required variables

Environment variables that are required can be specified using `:?` or `?` syntax:

```yaml
option: "${ENV_VAR:?err}" # Vector exits with 'err' message if variable is unset or empty
option: "${ENV_VAR?err}" # Vector exits with 'err' message only if variable is unset
```

#### Escaping

You can escape environment variables by prefacing them with a `$` character. For
example `$${HOSTNAME}` or `$$HOSTNAME` is treated literally in the above
environment variable example.

### Formats

Vector supports [YAML], [TOML], and [JSON] to ensure that Vector fits into your
workflow. A side benefit of supporting YAML and JSON is that they enable you to use
data templating languages such as [ytt], [Jsonnet] and [Cue].

#### Location

The location of your Vector configuration file depends on your installation
method. For most Linux-based systems, the file can be found at
`/etc/vector/vector.yaml`.

### Multiple files

You can pass multiple configuration files when starting Vector:

```shell
vector --config vector1.yaml --config vector2.yaml
```

Or using a [globbing syntax][glob]:

```shell
vector --config /etc/vector/*.yaml
```

#### Automatic namespacing

You can also split your configuration by grouping the components by their type, one directory per component type, where the file name is used as the component id. For example:

{{< tabs default="vector.yaml" >}}
{{< tab title="vector.yaml" >}}

```yaml
# Set global options
data_dir: "/var/lib/vector"

# Vector's API (disabled by default)
# Enable and try it out with the `vector top` command
api:
  enabled: false
  # address: "127.0.0.1:8686"
```

{{< /tab >}}
{{< tab title="sources/apache_logs.yaml" >}}

```yaml
# Ingest data by tailing one or more files
type: "file"
include: ["/var/log/apache2/*.log"]    # supports globbing
ignore_older: 86400                    # 1 day
```

{{< /tab >}}
{{< tab title="transforms/apache_parser.yaml" >}}

```yaml
# Structure and parse via Vector Remap Language
inputs:
  - "apache_logs"
type: "remap"
source: |
  . = parse_apache_log(.message)
```

{{< /tab >}}
{{< tab title="transforms/apache_sampler.yaml" >}}

```yaml
# Sample the data to save on cost
inputs:
  - "apache_parser"
type: "sample"
rate: 2 # only keep 50% (1/`rate`)
```

{{< /tab >}}
{{< tab title="sinks/es_cluster.yaml" >}}

```yaml
# Send structured data to a short-term storage
inputs:
  - "apache_sampler"             # only take sampled data
type: "elasticsearch"
endpoints:
  - "http://79.12.221.222:9200"  # local or external host
bulk:
  index: "vector-%Y-%m-%d"      # daily indices
```

{{< /tab >}}
{{< tab title="sinks/s3_archives.yaml" >}}

```yaml
# Send structured data to a cost-effective long-term storage
inputs:
  - "apache_parser"           # don't sample for S3
type: "aws_s3"
region: "us-east-1"
bucket: "my-log-archives"
key_prefix: "date=%Y-%m-%d"   # daily partitions, hive-friendly format
compression: "gzip"           # compress final objects
framing:
  method: "newline_delimited" # new line delimited...
encoding:
  codec: "json"               # ...JSON
batch:
  max_bytes: 10000000         # 10mb uncompressed
```

{{< /tab >}}
{{< /tabs >}}

Vector then needs to be started using the `--config-dir` argument to specify the root configuration folder.

```bash
vector --config-dir /etc/vector
```

#### Wildcards in component IDs

Vector supports wildcards (`*`) in component IDs when building your topology.
For example:

```yaml
sources:
  app1_logs:
    type: "file"
    includes: ["/var/log/app1.log"]

  app2_logs:
    type: "file"
    includes: ["/var/log/app.log"]

  system_logs:
    type: "file"
    includes: ["/var/log/system.log"]

sinks:
  app_logs:
    type: "datadog_logs"
    inputs: ["app*"]

  archive:
    type: "aws_s3"
    inputs: ["app*", "system_logs"]
```

### Enrichment tables

#### Memory enrichment table

Memory enrichment table has to be used as a sink to feed it data, which can then be queried like any
other enrichment table. The data has to conform to a specific format - the memory table will only
accept [VRL objects](/docs/reference/vrl/expressions/#object), where each key-value pair will be
stored as a separate entry in the table, associating the value with the key in the table. Value here
can be any VRL type.

```yaml
enrichment_tables:
  memory_table:
    type: memory
    ttl: 60
    flush_interval: 5
    inputs: ["cache_generator"]

sources:
  demo_logs_test:
    type: "demo_logs"
    format: "json"

transforms:
  demo_logs_processor:
    type: "remap"
    inputs: ["demo_logs_test"]
    source: |
      . = parse_json!(.message)
      user_id = get!(., path: ["user-identifier"])

      # Look for existing value in the table, using "user-identifier" as key
      existing, err = get_enrichment_table_record("memory_table", { "key": user_id })

      if err == null {
        # Value found, just use the cached value
        # In this case existing looks like this { "key": user_id, "value": {}, "ttl": 50 }
        # Where value is the value we cached, ttl is the time left before this value is removed from
        # the cache and key is the key we queried the table with
        . = existing.value
        .source = "cache"
      } else {
        # Do some processing, because we don't have this value in the table
        .referer = parse_url!(.referer)
        .referer.host = encode_punycode!(.referer.host)
        .source = "transform"
      }

  cache_generator:
    type: "remap"
    inputs: ["demo_logs_processor"]
    source: |
      existing, err = get_enrichment_table_record("memory_table", { "key": get!(., path: ["user-identifier"]) })
      if err != null {
        # We don't have this key cached, so we need to prepare it for the table
        data = .
        # Since the memory enrichment table takes in all key value pairs it receives and stores them
        # We want to produce an object that has the value of "user-identifier" as its key and
        # rest of the object as its value
        . = set!(value: {}, path: [get!(data, path: ["user-identifier"])], data: data)
      } else {
        . = {}
      }

# We can observe that after some time that some events have "source" set to "cache"
sinks:
  console:
    inputs: ["demo_logs_processor"]
    target: "stdout"
    type: "console"
    encoding:
      codec: "json"
```

#### Using memory enrichment table as a source

The memory enrichment table can serve as a source by periodically exporting its stored data and making it available to downstream components.

```yaml
enrichment_tables:
  memory_table:
    type: memory
    ttl: 60
    flush_interval: 5
    inputs: ["cache_generator"]
    source_config:
       # Export the cache every 3 minutes (100 seconds).
       export_interval: 180
       # If set to false (which is the default) it will not remove data from cache after exporting.
       remove_after_export: false
       # Source key has to be defined and be different from the main key ("memory_table").
       # This key is then used to define this component as an input to other components
       source_key: "memory_table_source"
       # export_batch_size can be used to reduce memory usage when handling larger tables.
       # When set, data will be exported from the table in batches, waiting for downstream components
       # to process the data
       # export_batch_size: 10000


sources:
  demo_logs_test:
    type: "demo_logs"
    format: "json"

transforms:
  demo_logs_processor:
    type: "remap"
    inputs: ["demo_logs_test"]
    source: |
      . = parse_json!(.message)
      user_id = get!(., path: ["user-identifier"])

      # Look for existing value in the table, using "user-identifier" as key
      existing, err = get_enrichment_table_record("memory_table", { "key": user_id })

      if err == null {
        # Value found, just use the cached value
        # In this case existing looks like this { "key": user_id, "value": {}, "ttl": 50 }
        # Where value is the value we cached, ttl is the time left before this value is removed from
        # the cache and key is the key we queried the table with
        . = existing.value
        .source = "cache"
      } else {
        # Do some processing, because we don't have this value in the table
        .referer = parse_url!(.referer)
        .referer.host = encode_punycode!(.referer.host)
        .source = "transform"
      }

  cache_generator:
    type: "remap"
    inputs: ["demo_logs_processor"]
    source: |
      existing, err = get_enrichment_table_record("memory_table", { "key": get!(., path: ["user-identifier"]) })
      if err != null {
        # We don't have this key cached, so we need to prepare it for the table
        data = .
        # Since the memory enrichment table takes in all key value pairs it receives and stores them
        # We want to produce an object that has the value of "user-identifier" as its key and
        # rest of the object as its value
        . = set!(value: {}, path: [get!(data, path: ["user-identifier"])], data: data)
      } else {
        . = {}
      }

# We can observe that after some time data will be exported to console from the cache
sinks:
  console:
    inputs: ["memory_table_source"]
    target: "stdout"
    type: "console"
    encoding:
      codec: "json"
```

## Sections

{{< sections >}}

## Pages

{{< pages >}}

[cue]: https://cuelang.org
[glob]: https://en.wikipedia.org/wiki/Glob_(programming)
[json]: https://json.org
[jsonnet]: https://jsonnet.org
[toml]: https://github.com/toml-lang/toml
[yaml]: https://yaml.org
[ytt]: https://carvel.dev/ytt/
