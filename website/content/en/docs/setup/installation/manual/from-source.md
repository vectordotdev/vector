---
title: Install Vector from source
short: From source
weight: 2
---

This page covers installing Vector from source using the native toolchain for the host.

Vector can also be compiled to a static binary for Linux for x86_64, ARM64, and ARMv7 architectures. See [compiling using Docker][docker] for details.

{{< warning >}}
We recommend installing Vector through a supported platform, package manager, or pre-built archive if possible. These handle permissions, directory creation, and other intricacies covered in the [Next Steps](#next-steps) section.
{{< /warning >}}

[docker]: /docs/setup/installation/manual/from-source/#docker

## Installation

### Linux

Install Rust:

```shell
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
```

Install compilation dependencies, specifically C and C++ compilers (GCC or Clang) and GNU `make` if they aren't pre-installed on your system.

Download Vector's source:

```shell
# Latest ({{< version >}})
mkdir -p vector && \
  curl -sSfL --proto '=https' --tlsv1.2 https://api.github.com/repos/timberio/vector/tarball/v{{< version >}} | \
  tar xzf - -C vector --strip-components=1

# Master
mkdir -p vector && \
  curl -sSfL --proto '=https' --tlsv1.2 https://github.com/timberio/vector/archive/master.tar.gz | \
  tar xzf - -C vector --strip-components=1
```

Change into your Vector directory:

```shell
cd vector
```

Compile Vector:

```shell
[FEATURES="<flag1>,<flag2>,..."] make build
```

The `FEATURES` environment variable is optional. You can override the default features using this variable. See [feature flags](#feature-flags) for more info.

When finished, the Vector binary is placed in `target/<target>/release/vector`. If you're building Vector on your Mac, for example, the target triple is `x86_64-apple-darwin` and the Vector binary will be located at `target/x86_64-apple-darwin/release/vector`.

Finally, you can start Vector:

```shell
target/<target>/release/vector --config config/vector.toml
```

### Windows

Install Rust using [`rustup`][rustup]. If you don't have VC++ build tools, the install will prompt you to install them.

Install [Perl for Windows][perl].

Add Perl to your `PATH`. In a Rust/MSVC environment (for example using `x64 Native Tools Command Prompt`) add the binary directory of Perl installed on the previous step to `PATH`. For example, for default installation of Strawberry Perl it is

```powershell
set PATH=%PATH%;C:\Strawberry\perl\bin
```

Get Vector's source using Git:

```shell
# Latest
git clone https://github.com/timberio/vector
git checkout v{{< version >}}
cd vector

# Master
git clone https://github.com/timberio/vector
cd vector
```

Build Vector in release mode:

```shell
set RUSTFLAGS=-Ctarget-feature=+crt-static
cargo build --no-default-features --features default-msvc --release
```

Start Vector. After these steps, a binary `vector.exe` in `target\release` would be created. It can be started by running:

```powershell
.\target\release\vector --config config\vector.toml
```

### Docker

You can build statically linked binaries of Vector for Linux using Docker. If you do so, the dependencies listed in the previous section aren't needed, as all of them would be automatically pulled by Docker.

First, download Vector's source:

```shell
# Latest ({{< version >}})
mkdir -p vector && \
  curl -sSfL --proto '=https' --tlsv1.2 https://api.github.com/repos/timberio/vector/tarball/v{{< version >}} | \
  tar xzf - -C vector --strip-components=1

# Master
mkdir -p vector && \
  curl -sSfL --proto '=https' --tlsv1.2 https://github.com/timberio/vector/archive/master.tar.gz | \
  tar xzf - -C vector --strip-components=1
```

Then build Vector using Docker:

```shell
# Linux (x86_64)
PASS_FEATURES=default-cmake ./scripts/docker-run.sh builder-x86_64-unknown-linux-musl make build

# Linux (ARM64)
PASS_FEATURES=default-cmake ./scripts/docker-run.sh builder-aarch64-unknown-linux-musl make build

# Linux (ARMv7)
PASS_FEATURES=default-cmake ./scripts/docker-run.sh builder-armv7-unknown-linux-musleabihf make build
```

The command above builds a Docker image with a Rust toolchain for a Linux target for the corresponding architecture using `musl` as the C library, then starts a container from this image, and then builds inside the container. The target binary is located at `target/<target triple>/release/vector` as in the previous case.

## Next steps

### Configuring

The Vector configuration file is located at:

```shell
config/vector.toml
```

A full spec is located at `config/vector.spec.toml` and examples are located in `config/vector/examples/*`. You can learn more about configuring Vector in the [Configuration] documentation.

### Data directory

We recommend creating a [data directory][data_dir] that Vector can use:

```shell
mkdir /var/lib/vector
```

{{< warning >}}
Make sure that this directory is writable by the `vector` process.
{{< /warning >}}

Vector offers a global [`data_dir` option][data_dir] that you can use to specify the path of your directory:

```shell
data_dir = "/var/lib/vector" # default
```

### Service managers

Vector archives ship with service files in case you need them:

#### Init.d

To install Vector into Init.d, run:

```shell
cp -av etc/init.d/vector /etc/init.d
```

#### Systemd

To install Vector into Systemd, run:

```shell
cp -av etc/systemd/vector.service /etc/systemd/system
```

### Updating

To update Vector, follow the same [installation](#installation) instructions above.

## How it works

### Feature flags

The following feature flags are supported via the `FEATURES` env var when executing `make build`:

```shell
[FEATURES="<flag1>,<flag2>,..."] make build
```

There are three meta-features that can be used when compiling for the corresponding targets. If no features are specified, the `default` is used.


Feature | Description | Enabled by default?
:-------|:------------|:-------------------
`default` | Default set of features for `*-unknown-linux-gnu` and `*-apple-darwin` targets. | ✅
`default-cmake` | Default set of features for `*-unknown-linux-*` targets which uses `cmake` and `perl` as build dependencies.
`default-msvc` | Default set of features for `*-pc-windows-msvc` targets. Requires `cmake` and `perl` as build dependencies.

Alternatively, for finer control over dependencies and operating system features, it is possible to use specific features from the list below:

Feature | Description | Included in `default` feature?
:-------|:------------|:------------------------------
`unix` | Enables features that require `cfg(unix)` to be present on the platform, namely support for Unix domain sockets in the [`docker_logs` source][docker_logs] and [jemalloc] instead of the default memory allocator. | ✅
`vendored` | Forces vendoring of [OpenSSL] and [ZLib] dependencies instead of using their versions installed in the system. Requires `perl` as a build dependency. | ✅
`leveldb-plain` | Enables support for [disk buffers][buffer] using vendored [LevelDB]. | ✅
`leveldb-cmake` | The same as `leveldb-plain`, but more portable. Requires `cmake` as a build dependency. Use this in case of compilation issues with `leveldb-plain`. |
`rdkafka-plain` | Enables vendored [`librdkafka`][librdkafka] dependency, which is required for the [`kafka` source][kafka_source] and [`kafka` sink][kafka_sink]. | ✅
`rdkafka-cmake` | The same as `rdkafka-plain` but more portable. Requires `cmake` as a build dependency. Use this in case of compilation issues with `rdkafka-plain`. |

In addition, it is possible to pick only a subset of Vector's components for the build using feature flags. In order to do it, it instead of default features one has to pass a comma-separated list of component features.

{{< details title="Click to see all component features" >}}
<!-- TODO: create a dedicated shortcode for this -->

**Vector component features**

Name | Description
:----|:-----------
| `sources-apache_metrics`                             | Enables building the [`apache_metrics` source](/docs/configuration/sources/apache_metrics)
| `sources-aws_kinesis_firehose`                       | Enables building the [`aws_kinesis_firehose` source](/docs/configuration/sources/aws_kinesis_firehose)
| `sources-docker_logs`                                | Enables building the [`docker_logs` source](/docs/configuration/sources/docker_logs). Requires `unix` feature to be also enabled for support of Unix domain sockets.
| `sources-file`                                       | Enables building the [`file` source](/docs/configuration/sources/files).
| `sources-generator`                                  | Enables building the [`generator` source](/docs/configuration/sources/generator)
| `sources-host_metrics`                               | Enables building the [`host_metrics` source](/docs/configuration/sources/host_metrics)
| `sources-http`                                       | Enables building the [`http` source](/docs/configuration/sources/http)
| `sources-journald`                                   | Enables building the [`journald` source](/docs/configuration/sources/journald)
| `sources-kafka`                                      | Enables building the [`kafka` source](/docs/configuration/sources/kafka). Requires `rdkafka-plain` or `rdkafka-cmake` feature to be also enabled.          |
| `sources-kubernetes_logs`                            | Enables building the [`kubernetes_logs` source](/docs/configuration/sources/kubernetes_logs)
| `sources-heroku_logs`                                | Enables building the [`heroku_logs` source](/docs/configuration/sources/heroku_logs)
| `sources-prometheus`                                 | Enables building the [`prometheus` source](/docs/configuration/sources/prometheus)
| `sources-socket`                                     | Enables building the [`socket` source](/docs/configuration/sources/socket)
| `sources-splunk_hec`                                 | Enables building the [`splunk_hec` source](/docs/configuration/sources/splunk_hec)
| `sources-statsd`                                     | Enables building the [`statsd` source](/docs/configuration/sources/statsd)
| `sources-stdin`                                      | Enables building the [`stdin` source](/docs/configuration/sources/stdin)
| `sources-syslog`                                     | Enables building the [`syslog` source](/docs/configuration/sources/syslog)
| `sources-vector`                                     | Enables building the [`vector` source](/docs/configuration/sources/vector)
| `transforms-add_fields`                              | Enables building the [`add_fields` transform](/docs/configuration/transforms/add_fields)
| `transforms-add_tags`                                | Enables building the [`add_tags` transform](/docs/configuration/transforms/add_tags)
| `transforms-ansi_stripper`                           | Enables building the [`ansi_stripper` transform](/docs/configuration/transforms/ansi_stripper)
| `transforms-aws_cloudwatch_logs_subscription_parser` | Enables building the [`aws_cloudwatch_logs_subscription_parser` transform](/docs/configuration/transforms/aws_cloudwatch_logs_subscription_parser)
| `transforms-aws_ec2_metadata`                        | Enables building of [`aws_ec2_metadata` transform](/docs/configuration/transforms/aws_ec2_metadata)
| `transforms-coercer`                                 | Enables building the [`coercer` transform](/docs/configuration/transforms/coercer)
| `transforms-concat`                                  | Enables building the [`concat` transform](/docs/configuration/transforms/concat)
| `transforms-dedupe`                                  | Enables building the [`dedupe` transform](/docs/configuration/transforms/dedupe)
| `transforms-filter`                                  | Enables building the [`filter` transform](/docs/configuration/transforms/filter)
| `transforms-geoip`                                   | Enables building the [`geoip` transform](/docs/configuration/transforms/geoip)
| `transforms-grok_parser`                             | Enables building the [`grok_parser` transform](/docs/configuration/transforms/grok_parser)
| `transforms-json_parser`                             | Enables building the [`json_parser` transform](/docs/configuration/transforms/json_parser)
| `transforms-log_to_metric`                           | Enables building the [`log_to_metric` transform](/docs/configuration/transforms/log_to_metric)
| `transforms-logfmt_parser`                           | Enables building the [`logfmt_parser` transform](/docs/configuration/transforms/logfmt_parser)
| `transforms-lua`                                     | Enables building the [`lua` transform](/docs/configuration/transforms/lua)
| `transforms-merge`                                   | Enables building the [`merge` transform](/docs/configuration/transforms/merge)
| `transforms-metric_to_log`                           | Enables building the [`metric_to_log` transform](/docs/configuration/transforms/metric_to_log)
| `transforms-reduce`                                  | Enables building the [`reduce` transform](/docs/configuration/transforms/reduce)
| `transforms-regex_parser`                            | Enables building the [`regex_parser` transform](/docs/configuration/transforms/regex_parser)
| `transforms-remap`                                   | Enables building the [`remap` transform](/docs/configuration/transforms/remap)
| `transforms-remove_fields`                           | Enables building the [`remove_fields` transform](/docs/configuration/transforms/remove_fields)
| `transforms-remove_tags`                             | Enables building the [`remove_tags` transform](/docs/configuration/transforms/remove_tags)
| `transforms-rename_fields`                           | Enables building the [`rename_fields` transform](/docs/configuration/transforms/rename_fields)
| `transforms-sample`                                  | Enables building the [`sample` transform](/docs/configuration/transforms/sample)
| `transforms-split`                                   | Enables building the [`split` transform](/docs/configuration/transforms/split)
| `transforms-route`                                   | Enables building the [`route` transform](/docs/configuration/transforms/route)
| `transforms-tag_cardinality_limit`                   | Enables building the [`tag_cardinality_limit` transform](/docs/configuration/transforms/tag_cardinality_limit)
| `transforms-tokenizer`                               | Enables building the [`tokenizer` transform](/docs/configuration/transforms/tokenizer)
| `transforms-wasm`                                    | Enables building the [`wasm` transform](/docs/configuration/transforms/wasm)
| `sinks-aws_cloudwatch_logs`                          | Enables building the [`aws_cloudwatch_logs` sink](/docs/configuration/sinks/aws_cloudwatch_logs)
| `sinks-aws_cloudwatch_metrics`                       | Enables building the [`aws_cloudwatch_metrics` sink](/docs/configuration/sinks/aws_cloudwatch_metrics)
| `sinks-aws_kinesis_firehose`                         | Enables building the [`aws_kinesis_firehose` sink](/docs/configuration/sinks/aws_kinesis_firehose)
| `sinks-aws_kinesis_streams`                          | Enables building the [`aws_kinesis_streams` sink](/docs/configuration/sinks/aws_kinesis_streams)
| `sinks-aws_s3`                                       | Enables building the [`aws_s3` sink](/docs/configuration/sinks/aws_s3)
| `sinks-azure_monitor_logs`                           | Enables building the [`azure_monitor_logs` sink](/docs/configuration/sinks/azure_monitor_logs)
| `sinks-blackhole`                                    | Enables building the [`blackhole` sink](/docs/configuration/sinks/blackhole)
| `sinks-clickhouse`                                   | Enables building the [`clickhouse` sink](/docs/configuration/sinks/clickhouse)
| `sinks-console`                                      | Enables building the [`console` sink](/docs/configuration/sinks/console)
| `sinks-datadog_logs`                                 | Enables building the [`datadog_logs` sink](/docs/configuration/sinks/datadog_logs)
| `sinks-datadog_metrics`                              | Enables building the [`datadog_metrics` sink](/docs/configuration/sinks/datadog_metrics)
| `sinks-elasticsearch`                                | Enables building the [`elasticsearch` sink](/docs/configuration/sinks/elasticsearch)
| `sinks-file`                                         | Enables building the [`file` sink](/docs/configuration/sinks/file)
| `sinks-gcp_cloud_storage`                            | Enables building the [`gcp_cloud_storage` sink](/docs/configuration/sinks/gcp_cloud_storage)
| `sinks-gcp_pubsub`                                   | Enables building the [`gcp_pubsub` sink](/docs/configuration/sinks/gcp_pubsub)
| `sinks-gcp_stackdriver_logs`                         | Enables building the [`gcp_stackdriver_logs` sink](/docs/configuration/sinks/gcp_stackdriver_logs)
| `sinks-honeycomb`                                    | Enables building the [`honeycomb` sink](/docs/configuration/sinks/honeycomb)
| `sinks-http`                                         | Enables building the [`http` sink](/docs/configuration/sinks/http)
| `sinks-humio_logs`                                   | Enables building the [`humio_logs` sink](/docs/configuration/sinks/humio_logs)
| `sinks-humio_metrics`                                | Enables building the [`humio_metrics` sink](/docs/configuration/sinks/humio_metrics)
| `sinks-influxdb_logs`                                | Enables building the [`influxdb_logs` sink](/docs/configuration/sinks/influxdb_logs)
| `sinks-influxdb_metrics`                             | Enables building the [`influxdb_metrics` sink](/docs/configuration/sinks/influxdb_metrics)
| `sinks-kafka`                                        | Enables building the [`kafka` sink](/docs/configuration/sinks/kafka). Requires `rdkafka-plain` or `rdkafka-cmake` feature to be also enabled.
| `sinks-logdna`                                       | Enables building the [`logdna` sink](/docs/configuration/sinks/logdna)
| `sinks-loki`                                         | Enables building the [`loki` sink](/docs/configuration/sinks/loki)
| `sinks-new_relic_logs`                               | Enables building the [`new_relic_logs` sink](/docs/configuration/sinks/new_relic_logs)
| `sinks-papertrail`                                   | Enables building the [`papertrail` sink](/docs/configuration/sinks/papertrail)
| `sinks-prometheus`                                   | Enables building the [`prometheus` sink](/docs/configuration/sinks/prometheus)
| `sinks-pulsar`                                       | Enables building the [`pulsar` sink](/docs/configuration/sinks/pulsar)
| `sinks-sematext_logs`                                | Enables building the [`sematext_logs` sink](/docs/configuration/sinks/sematext_logs)
| `sinks-sematext_metrics`                             | Enables building the [`sematext_metrics` sink](/docs/configuration/sinks/sematext_metrics)
| `sinks-socket`                                       | Enables building the [`socket` sink](/docs/configuration/sinks/socket)
| `sinks-splunk_hec`                                   | Enables building the [`splunk_hec` sink](/docs/configuration/sinks/splunk_hec)
| `sinks-statsd`                                       | Enables building the [`statsd` sink](/docs/configuration/sinks/statsd)
| `sinks-vector`                                       | Enables building the [`vector` sink](/docs/configuration/sinks/vector)
{{< /details >}}

[buffer]: /docs/meta/glossary/#buffer
[configuration]: /docs/reference/configuration
[data_dir]: /docs/reference/configuration/global-options/#data_dir
[docker_logs]: /docs/reference/configuration/sources/docker_logs
[jemalloc]: https://github.com/jemalloc/jemalloc
[kafka_sink]: /docs/reference/configuration/sinks/kafka
[kafka_source]: /docs/reference/configuration/sources/kafka
[leveldb]: https://github.com/google/leveldb
[librdkafka]: https://github.com/edenhill/librdkafka
[openssl]: https://www.openssl.org
[perl]: https://www.perl.org/get.html#win32
[rustup]: https://rustup.rs
[zlib]: https://www.zlib.net
