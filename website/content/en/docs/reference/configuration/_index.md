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

{{< tabs default="vector.toml" >}} {{< tab title="vector.toml" >}}

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
type         = "file"
include      = ["/var/log/apache2/*.log"]    # supports globbing
ignore_older = 86400                         # 1 day

# Structure and parse via Timber's Remap Language
[transforms.remap]
inputs       = ["apache_logs"]
type         = "remap"
source       = '''
. = parse_apache_log(.message)
'''

# Sample the data to save on cost
[transforms.apache_sampler]
inputs       = ["apache_parser"]
type         = "sampler"
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
compression  = "gzip"                        # compress final objects
encoding     = "ndjson"                      # new line delimited JSON
batch.max_size   = 10000000                  # 10mb uncompressed
```

{{< /tab >}} {{< tab title="vector.yaml" >}}

```yaml
data_dir: /var/lib/vector
sources:
  apache_logs:
    type: file
    include:
      - /var/log/apache2/*.log
    ignore_older: 86400
transforms:
  remap:
    inputs:
      - apache_logs
    type: remap
    source: |
      . = parse_apache_log(.message)
  apache_sampler:
    inputs:
      - apache_parser
    type: sampler
    rate: 50
sinks:
  es_cluster:
    inputs:
      - apache_sampler
    type: elasticsearch
    host: 'http://79.12.221.222:9200'
    index: vector-%Y-%m-%d
  s3_archives:
    inputs:
      - apache_parser
    type: aws_s3
    region: us-east-1
    bucket: my-log-archives
    key_prefix: date=%Y-%m-%d
    compression: gzip
    encoding: ndjson
    batch:
      max_size: 10000000
```

{{< /tab >}} {{< tab title="vector.json" >}}

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
    "remap": {
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
      "type": "sampler",
      "rate": 50
    }
  },
  "sinks": {
    "es_cluster": {
      "inputs": [
        "apache_sampler"
      ],
      "type": "elasticsearch",
      "host": "http://79.12.221.222:9200",
      "index": "vector-%Y-%m-%d"
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
      "encoding": "ndjson",
      "batch": {
        "max_size": 10000000
      }
    }
  }
}
```

{{< /tab >}} {{< /tabs >}}

To use this configuration file, specify it with the `--config` flag when
starting Vector:

{{< tabs default="TOML" >}} {{< tab title="TOML" >}}

```shell
vector --config /etc/vector/vector.toml
```

{{< /tab >}} {{< tab title="YAML" >}}

```shell
vector --config /etc/vector/vector.yaml
```

{{< /tab >}} {{< tab title="JSON" >}}

```shell
vector --config /etc/vector/vector.json
```

{{< /tab >}} {{< /tabs >}}

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

```toml
[transforms.add_host]
type = "add_fields"

[transforms.add_host.fields]
host = "${HOSTNAME}" # or "$HOSTNAME"
environment = "${ENV:-development}" # default value when not present
```

#### Default values

Default values can be supplied using `:-` syntax:

```toml
option = "${ENV_VAR:-default}"
```

#### Escaping

You can escape environment variables by prefacing them with a `$` character. For
example `$${HOSTNAME}` or `$$HOSTNAME` is treated literally in the above
environment variable example.

### Formats

Vector supports [TOML], [YAML], and [JSON] to ensure that Vector fits into your
workflow. A side benefit of supporting JSON is that it enables you to use
JSON-outputting data templating languages like [Jsonnet] and [Cue].

#### Location

The location of your Vector configuration file depends on your installation
method. For most Linux-based systems, the file can be found at
`/etc/vector/vector.toml`.

### Multiple files

You can pass multiple configuration files when starting Vector:

```shell
vector --config vector1.toml --config vector2.toml
```

Or using a [globbing syntax][glob]:

```shell
vector --config /etc/vector/*.toml
```

#### Wilcards in component IDs

Vector supports wildcards (`*`) in component IDs when building your topology.
For example:

```toml
[sources.app1_logs]
type = "file"
includes = ["/var/log/app1.log"]

[sources.app2_logs]
type = "file"
includes = ["/var/log/app.log"]

[sources.system_logs]
type = "file"
includes = ["/var/log/system.log"]

[sinks.app_logs]
type = "datadog_logs"
inputs = ["app*"]

[sinks.archive]
type = "aws_s3"
inputs = ["app*", "system_logs"]
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
