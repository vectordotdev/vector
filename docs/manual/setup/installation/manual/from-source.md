---
title: Install Vector From Source
sidebar_label: From Source
description: Install Vector from the Vector source code
---

This page covers installing Vector from source using the native toolchain for
the host.

Vector can also be compiled to a static binary for Linux for `x86_64`, `ARM64`,
and `ARMv7` architectures. See [compiling using Docker](#compiling-using-docker)
for details.

<Alert type="warning">

We recommend installing Vector through a supported [platform][docs.platforms],
[package manager][docs.package_managers], or pre-built
[archive][docs.from_archives], if possible. These handle permissions, directory
creation, and other intricacies covered in the [Next Steps](#next-steps)
section.

</Alert>

## Installation

<Tabs
block={true}
defaultValue="linux"
values={[
{ label: 'Linux', value: 'linux'},
{ label: 'Windows', value: 'windows'},
{ label: 'Docker', value: 'docker'},
]}>

<TabItem value="linux">

The following steps should be used to compile Vector directly on Linux based systems.

<Steps headingDepth={3}>

1.  ### Install Rust

    ```bash
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    ```

2.  ### Install compilation dependencies

    Install C and C++ compilers (GCC or Clang) and GNU `make` if they are not
    pre-installed on your system.

3.  ### Download Vector's Source

    <Tabs
    className="mini"
    defaultValue="latest"
    values={[
    { label: 'Latest (0.10.0)', value: 'latest'},
    { label: 'Master', value: 'master'},
    ]}>

    <TabItem value="latest">

    ```bash
    mkdir -p vector && \
      curl -sSfL --proto '=https' --tlsv1.2 https://api.github.com/repos/timberio/vector/tarball/v0.10 | \
      tar xzf - -C vector --strip-components=1
    ```

    </TabItem>
    <TabItem value="master">

    ```bash
    mkdir -p vector && \
      curl -sSfL --proto '=https' --tlsv1.2 https://github.com/timberio/vector/archive/master.tar.gz | \
      tar xzf - -C vector --strip-components=1
    ```

    </TabItem>
    </Tabs>

4.  ### Change into the `vector` directory

    ```bash
    cd vector
    ```

5.  ### Compile Vector

    ```bash
    [FEATURES="<flag1>,<flag2>,..."] make build
    ```

    The `FEATURES` environment variable is optional. You can override the
    default features with this variable. See [feature flags](#feature-flags)
    for more info.

    When finished, the vector binary will be placed in
    `target/<target>/release/vector`. For example, if you are building Vector
    on your Mac, your target triple is `x86_64-apple-darwin`, and the Vector
    binary will be located at `target/x86_64-apple-darwin/release/vector`.

6.  ### Start Vector

    Finally, start vector:

    ```bash
    target/<target>/release/vector --config config/vector.toml
    ```

</Steps>
</TabItem>
<TabItem value="windows">

The steps to compile Vector on Windows are different from the ones for other
operating systems.

<Steps headingDepth={3}>

1. ### Install Rust

   Install Rust using [`rustup`][urls.rustup]. If you don't have VC++ build
   tools, the installer would prompt you to install them.

2. ### Install Perl

   Install [Perl for Windows][urls.perl_windows].

3. ### Add Perl to your `PATH`

   In a Rust/MSVC environment (for example, using
   `x64 Native Tools Command Prompt`) add the binary directory of Perl
   installed on the previous step to `PATH`. For example, for default
   installation of Strawberry Perl it is

   ```bat
   set PATH=%PATH%;C:\Strawberry\perl\bin
   ```

4. ### Get Vector's source using `git`

   <Tabs
   className="mini"
   defaultValue="latest"
   values={[
   { label: 'Latest (0.10.0)', value: 'latest'},
   { label: 'Master', value: 'master'},
   ]}>

   <TabItem value="latest">

   ```bat
   git clone https://github.com/timberio/vector
   git checkout v0.10.0
   cd vector
   ```

   </TabItem>
   <TabItem value="master">

   ```bat
   git clone https://github.com/timberio/vector
   cd vector
   ```

   </TabItem>
   </Tabs>

5. ### Build Vector in release mode

   ```bat
   set RUSTFLAGS=-Ctarget-feature=+crt-static
   cargo build --no-default-features --features default-msvc --release
   ```

6. ### Start Vector

   After these steps a binary `vector.exe` in `target\release` would be
   created. It can be started by running

   ```bat
   .\target\release\vector --config config\vector.toml
   ```

</Steps>
</TabItem>
<TabItem value="docker">

It is possible to build statically linked binaries of Vector for Linux using
Docker.

In this case the dependencies listed in the previous section are not
needed, as all of them would be automatically pulled by Docker.

Building steps:

<Steps headingDepth={3}>

1.  ### Download Vector's Source

    <Tabs
    className="mini"
    defaultValue="latest"
    values={[
    { label: 'Latest (0.10.0)', value: 'latest'},
    { label: 'Master', value: 'master'},
    ]}>

    <TabItem value="latest">

    ```bash
    mkdir -p vector && \
      curl -sSfL --proto '=https' --tlsv1.2 https://api.github.com/repos/timberio/vector/tarball/v0.10.X | \
      tar xzf - -C vector --strip-components=1
    ```

    </TabItem>
    <TabItem value="master">

    ```bash
    mkdir -p vector && \
      curl -sSfL --proto '=https' --tlsv1.2 https://github.com/timberio/vector/archive/master.tar.gz | \
      tar xzf - -C vector --strip-components=1
    ```

    </TabItem>
    </Tabs>

2.  ### Build Vector using Docker

    <Tabs
    defaultValue="x86_64-unknown-linux-musl"
    urlKey="file_name"
    values={
    [{
    "label":"Linux (x86_64)",
    "value":"x86_64-unknown-linux-musl"
    }, {
    "label":"Linux (ARM64)",
    "value":"aarch64-unknown-linux-musl"
    },{
    "label":"Linux (ARMv7)",
    "value":"armv7-unknown-linux-musleabihf"
    }]
    }>

    <TabItem value="x86_64-unknown-linux-musl">

    ```bash
    PASS_FEATURES=default-cmake ./scripts/docker-run.sh builder-x86_64-unknown-linux-musl make build
    ```

    </TabItem>

    <TabItem value="aarch64-unknown-linux-musl">

    ```bash
    PASS_FEATURES=default-cmake ./scripts/docker-run.sh builder-aarch64-unknown-linux-musl make build
    ```

    </TabItem>

    <TabItem value="armv7-unknown-linux-musleabihf">

    ```bash
    PASS_FEATURES=default-cmake ./scripts/docker-run.sh builder-armv7-unknown-linux-musleabihf make build
    ```

    </TabItem>
    </Tabs>

    The command above builds a Docker image with Rust toolchain for a Linux
    target for the corresponding architecture using `musl` as the C library,
    then starts a container from this image, and then builds inside the
    Container. The target binary is located in
    `target/<target triple>/release/vector` like in the previous case.

</Steps>
</TabItem>
</Tabs>

## Next Steps

### Configuring

The Vector configuration file is located at:

```text
config/vector.toml
```

Example configurations are located in `config/vector/examples/*`. You can learn more about configuring
Vector in the [Configuration][docs.setup.configuration] section.

### Data Directory

We highly recommend creating a [data directory][docs.global-options#data_dir]
that Vector can use:

```bash
mkdir /var/lib/vector
```

<Alert type="warning">

Make sure that this directory is writable by the `vector` process.

</Alert>

Vector offers a global [`data_dir` option][docs.global-options#data_dir] that
you can use to specify the path of your directory.

```toml title="vector.toml"
data_dir = "/var/lib/vector" # default
```

### Service Managers

Vector archives ship with service files in case you need them:

#### Init.d

To install Vector into Init.d run:

```bash
cp -av distribution/init.d/vector /etc/init.d
```

#### Systemd

To install Vector into Systemd run:

```bash
cp -av distribution/systemd/vector.service /etc/systemd/system
```

### Updating

Simply follow the same [installation instructions above](#installation).

## How It Works

### Feature Flags

The following feature flags are supported via the `FEATURES` env var when
executing `make build`:

```bash
[FEATURES="<flag1>,<flag2>,..."] make build
```

There are three meta-features which can be used when compiling for the
corresponding targets. If no features are specified, then the `default` one is
used.

| Feature         | Description                                                                                                  | Enabled by default                     |
| :-------------- | :----------------------------------------------------------------------------------------------------------- | :------------------------------------- |
| `default`       | Default set of features for `*-unknown-linux-gnu` and `*-apple-darwin` targets.                              | <i className="feather icon-check"></i> |
| `default-cmake` | Default set of features for `*-unknown-linux-*` targets which uses `cmake` and `perl` as build dependencies. |                                        |
| `default-msvc`  | Default set of features for `*-pc-windows-msvc` targets. Requires `cmake` and `perl` as build dependencies.  |                                        |

Alternatively, for finer control over dependencies and operating system
features, it is possible to use specific features from the list below:

| Feature         | Description                                                                                                                                                                                                                    | Included in `default` feature          |
| :-------------- | :----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :------------------------------------- |
| `unix`          | Enables features that require `cfg(unix)` to be present on the platform, namely support for Unix domain sockets in [docker][docs.sources.docker] source and [jemalloc][urls.jemalloc] instead of the default memory allocator. | <i className="feather icon-check"></i> |
| `vendored`      | Forces vendoring of [OpenSSL][urls.openssl] and [ZLib][urls.zlib] dependencies instead of using their versions installed in the system. Requires `perl` as a build dependency.                                                 | <i className="feather icon-check"></i> |
| `leveldb-plain` | Enables support for [disk buffers][docs.glossary#buffer] using vendored [LevelDB][urls.leveldb].                                                                                                                               | <i className="feather icon-check"></i> |
| `leveldb-cmake` | The same as `leveldb-plain`, but is more portable. Requires `cmake` as a build dependency. Use it in case of compilation issues with `leveldb-plain`.                                                                          |                                        |
| `rdkafka-plain` | Enables vendored [librdkafka][urls.librdkafka] dependency, which is required for [`kafka` source][docs.sources.kafka] and [`kafka` sink][docs.sources.kafka].                                                                  | <i className="feather icon-check"></i> |
| `rdkafka-cmake` | The same as `rdkafka-plain`, but is more portable. Requires `cmake` as a build dependency. Use it in case of compilation issues with `rdkafka-plain`.                                                                          |                                        |

In addition, it is possible to pick only a subset of Vector's components for
the build using feature flags. In order to do it, it instead of `default`
features one has to pass a comma-separated list of component features.

<details><summary>Click to see all supported component features</summary>
<p>

| Name                                                 | Description                                                                                                                                |
| :--------------------------------------------------- | :----------------------------------------------------------------------------------------------------------------------------------------- |
| `sources-apache_metrics`                             | Enables building of [`apache_metrics` source][docs.sources.apache_metrics].                                                                |
| `sources-aws_kinesis_firehose`                       | Enables building of [`aws_kinesis_firehose` source][docs.sources.aws_kinesis_firehose].                                                    |
| `sources-docker_logs`                                | Enables building of [`docker_logs` source][docs.sources.docker_logs]. Requires `unix` feature to be also enabled for support of Unix domain sockets. |
| `sources-file`                                       | Enables building of [`file` source][docs.sources.file].                                                                                    |
| `sources-generator`                                  | Enables building of [`generator` source][docs.sources.generator].                                                                          |
| `sources-host_metrics`                               | Enables building of [`host_metrics` source][docs.sources.host_metrics].                                                                    |
| `sources-http`                                       | Enables building of [`http` source][docs.sources.http].                                                                                    |
| `sources-journald`                                   | Enables building of [`journald` source][docs.sources.journald].                                                                            |
| `sources-kafka`                                      | Enables building of [`kafka` source][docs.sources.kafka]. Requires `rdkafka-plain` or `rdkafka-cmake` feature to be also enabled.          |
| `sources-kubernetes_logs`                            | Enables building of [`kubernetes_logs` source][docs.sources.kubernetes_logs].                                                              |
| `sources-heroku_logs`                                | Enables building of [`heroku_logs` source][docs.sources.heroku_logs].                                                                              |
| `sources-prometheus`                                 | Enables building of [`prometheus` source][docs.sources.prometheus].                                                                        |
| `sources-socket`                                     | Enables building of [`socket` source][docs.sources.socket].                                                                                |
| `sources-splunk_hec`                                 | Enables building of [`splunk_hec` source][docs.sources.splunk_hec].                                                                        |
| `sources-statsd`                                     | Enables building of [`statsd` source][docs.sources.statsd].                                                                                |
| `sources-stdin`                                      | Enables building of [`stdin` source][docs.sources.stdin].                                                                                  |
| `sources-syslog`                                     | Enables building of [`syslog` source][docs.sources.syslog].                                                                                |
| `sources-vector`                                     | Enables building of [`vector` source][docs.sources.vector].                                                                                |
| `transforms-add_fields`                              | Enables building of [`add_fields` transform][docs.transforms.add_fields].                                                                  |
| `transforms-add_tags`                                | Enables building of [`add_tags` transform][docs.transforms.add_tags].                                                                      |
| `transforms-ansi_stripper`                           | Enables building of [`ansi_stripper` transform][docs.transforms.ansi_stripper].                                                            |
| `transforms-aws_cloudwatch_logs_subscription_parser` | Enables building of [`aws_cloudwatch_logs_subscription_parser` transform][docs.transforms.aws_cloudwatch_logs_subscription_parser].        |
| `transforms-aws_ec2_metadata`                        | Enables building of [`aws_ec2_metadata` transform][docs.transforms.aws_ec2_metadata].                                                      |
| `transforms-coercer`                                 | Enables building of [`coercer` transform][docs.transforms.coercer].                                                                        |
| `transforms-concat`                                  | Enables building of [`concat` transform][docs.transforms.concat].                                                                          |
| `transforms-dedupe`                                  | Enables building of [`dedupe` transform][docs.transforms.dedupe].                                                                          |
| `transforms-filter`                                  | Enables building of [`filter` transform][docs.transforms.filter].                                                                          |
| `transforms-geoip`                                   | Enables building of [`geoip` transform][docs.transforms.geoip].                                                                            |
| `transforms-grok_parser`                             | Enables building of [`grok_parser` transform][docs.transforms.grok_parser].                                                                |
| `transforms-json_parser`                             | Enables building of [`json_parser` transform][docs.transforms.json_parser].                                                                |
| `transforms-log_to_metric`                           | Enables building of [`log_to_metric` transform][docs.transforms.log_to_metric].                                                            |
| `transforms-logfmt_parser`                           | Enables building of [`logfmt_parser` transform][docs.transforms.logfmt_parser].                                                            |
| `transforms-lua`                                     | Enables building of [`lua` transform][docs.transforms.lua].                                                                                |
| `transforms-merge`                                   | Enables building of [`merge` transform][docs.transforms.merge].                                                                            |
| `transforms-metric_to_log`                           | Enables building of [`metric_to_log` transform][docs.transforms.metric_to_log].                                                            |
| `transforms-reduce`                                  | Enables building of [`reduce` transform][docs.transforms.reduce].                                                                          |
| `transforms-regex_parser`                            | Enables building of [`regex_parser` transform][docs.transforms.regex_parser].                                                              |
| `transforms-remap`                                   | Enables building of [`remap` transform][docs.transforms.remap].                                                                            |
| `transforms-remove_fields`                           | Enables building of [`remove_fields` transform][docs.transforms.remove_fields].                                                            |
| `transforms-remove_tags`                             | Enables building of [`remove_tags` transform][docs.transforms.remove_tags].                                                                |
| `transforms-rename_fields`                           | Enables building of [`rename_fields` transform][docs.transforms.rename_fields].                                                            |
| `transforms-sample`                                  | Enables building of [`sample` transform][docs.transforms.sample].                                                                        |
| `transforms-split`                                   | Enables building of [`split` transform][docs.transforms.split].                                                                            |
| `transforms-route`                                   | Enables building of [`route` transform][docs.transforms.route].                                                                    |
| `transforms-tag_cardinality_limit`                   | Enables building of [`tag_cardinality_limit` transform][docs.transforms.tag_cardinality_limit].                                            |
| `transforms-tokenizer`                               | Enables building of [`tokenizer` transform][docs.transforms.tokenizer].                                                                    |
| `transforms-wasm`                                    | Enables building of [`wasm` transform][docs.transforms.wasm].                                                                              |
| `sinks-aws_cloudwatch_logs`                          | Enables building of [`aws_cloudwatch_logs` sink][docs.sinks.aws_cloudwatch_logs].                                                          |
| `sinks-aws_cloudwatch_metrics`                       | Enables building of [`aws_cloudwatch_metrics` sink][docs.sinks.aws_cloudwatch_metrics].                                                    |
| `sinks-aws_kinesis_firehose`                         | Enables building of [`aws_kinesis_firehose` sink][docs.sinks.aws_kinesis_firehose].                                                        |
| `sinks-aws_kinesis_streams`                          | Enables building of [`aws_kinesis_streams` sink][docs.sinks.aws_kinesis_streams].                                                          |
| `sinks-aws_s3`                                       | Enables building of [`aws_s3` sink][docs.sinks.aws_s3].                                                                                    |
| `sinks-azure_monitor_logs`                           | Enables building of [`azure_monitor_logs` sink][docs.sinks.azure_monitor_logs].                                                            |
| `sinks-blackhole`                                    | Enables building of [`blackhole` sink][docs.sinks.blackhole].                                                                              |
| `sinks-clickhouse`                                   | Enables building of [`clickhouse` sink][docs.sinks.clickhouse].                                                                            |
| `sinks-console`                                      | Enables building of [`console` sink][docs.sinks.console].                                                                                  |
| `sinks-datadog_logs`                                 | Enables building of [`datadog_logs` sink][docs.sinks.datadog_logs].                                                                        |
| `sinks-datadog_metrics`                              | Enables building of [`datadog_metrics` sink][docs.sinks.datadog_metrics].                                                                  |
| `sinks-elasticsearch`                                | Enables building of [`elasticsearch` sink][docs.sinks.elasticsearch].                                                                      |
| `sinks-file`                                         | Enables building of [`file` sink][docs.sinks.file].                                                                                        |
| `sinks-gcp_cloud_storage`                            | Enables building of [`gcp_cloud_storage` sink][docs.sinks.gcp_cloud_storage].                                                              |
| `sinks-gcp_pubsub`                                   | Enables building of [`gcp_pubsub` sink][docs.sinks.gcp_pubsub].                                                                            |
| `sinks-gcp_stackdriver_logs`                         | Enables building of [`gcp_stackdriver_logs` sink][docs.sinks.gcp_stackdriver_logs].                                                        |
| `sinks-honeycomb`                                    | Enables building of [`honeycomb` sink][docs.sinks.honeycomb].                                                                              |
| `sinks-http`                                         | Enables building of [`http` sink][docs.sinks.http].                                                                                        |
| `sinks-humio_logs`                                   | Enables building of [`humio_logs` sink][docs.sinks.humio_logs].                                                                            |
| `sinks-humio_metrics`                                | Enables building of [`humio_metrics` sink][docs.sinks.humio_metrics].                                                                      |
| `sinks-influxdb_logs`                                | Enables building of [`influxdb_logs` sink][docs.sinks.influxdb_logs].                                                                      |
| `sinks-influxdb_metrics`                             | Enables building of [`influxdb_metrics` sink][docs.sinks.influxdb_metrics].                                                                |
| `sinks-kafka`                                        | Enables building of [`kafka` sink][docs.sinks.kafka]. Requires `rdkafka-plain` or `rdkafka-cmake` feature to be also enabled.              |
| `sinks-logdna`                                       | Enables building of [`logdna` sink][docs.sinks.logdna].                                                                                    |
| `sinks-loki`                                         | Enables building of [`loki` sink][docs.sinks.loki].                                                                                        |
| `sinks-new_relic_logs`                               | Enables building of [`new_relic_logs` sink][docs.sinks.new_relic_logs].                                                                    |
| `sinks-papertrail`                                   | Enables building of [`papertrail` sink][docs.sinks.papertrail].                                                                            |
| `sinks-prometheus`                                   | Enables building of [`prometheus` sink][docs.sinks.prometheus].                                                                            |
| `sinks-pulsar`                                       | Enables building of [`pulsar` sink][docs.sinks.pulsar].                                                                                    |
| `sinks-sematext_logs`                                | Enables building of [`sematext_logs` sink][docs.sinks.sematext_logs].                                                                      |
| `sinks-sematext_metrics`                             | Enables building of [`sematext_metrics` sink][docs.sinks.sematext_metrics].                                                                |
| `sinks-socket`                                       | Enables building of [`socket` sink][docs.sinks.socket].                                                                                    |
| `sinks-splunk_hec`                                   | Enables building of [`splunk_hec` sink][docs.sinks.splunk_hec].                                                                            |
| `sinks-statsd`                                       | Enables building of [`statsd` sink][docs.sinks.statsd].                                                                                    |
| `sinks-vector`                                       | Enables building of [`vector` sink][docs.sinks.vector].                                                                                    |

</p>
</details>

[docs.setup.configuration]: /docs/setup/configuration/
[docs.from_archives]: /docs/setup/installation/manual/from-archives/
[docs.global-options#data_dir]: /docs/reference/global-options/#data_dir
[docs.glossary#buffer]: /docs/meta/glossary/#buffer
[docs.package_managers]: /docs/setup/installation/package-managers/
[docs.platforms]: /docs/setup/installation/platforms/
[docs.sinks.aws_cloudwatch_logs]: /docs/reference/sinks/aws_cloudwatch_logs/
[docs.sinks.aws_cloudwatch_metrics]: /docs/reference/sinks/aws_cloudwatch_metrics/
[docs.sinks.aws_kinesis_firehose]: /docs/reference/sinks/aws_kinesis_firehose/
[docs.sinks.aws_kinesis_streams]: /docs/reference/sinks/aws_kinesis_streams/
[docs.sinks.aws_s3]: /docs/reference/sinks/aws_s3/
[docs.sinks.azure_monitor_logs]: /docs/reference/sinks/azure_monitor_logs/
[docs.sinks.blackhole]: /docs/reference/sinks/blackhole/
[docs.sinks.clickhouse]: /docs/reference/sinks/clickhouse/
[docs.sinks.console]: /docs/reference/sinks/console/
[docs.sinks.datadog_logs]: /docs/reference/sinks/datadog_logs/
[docs.sinks.datadog_metrics]: /docs/reference/sinks/datadog_metrics/
[docs.sinks.elasticsearch]: /docs/reference/sinks/elasticsearch/
[docs.sinks.file]: /docs/reference/sinks/file/
[docs.sinks.gcp_cloud_storage]: /docs/reference/sinks/gcp_cloud_storage/
[docs.sinks.gcp_pubsub]: /docs/reference/sinks/gcp_pubsub/
[docs.sinks.gcp_stackdriver_logs]: /docs/reference/sinks/gcp_stackdriver_logs/
[docs.sinks.honeycomb]: /docs/reference/sinks/honeycomb/
[docs.sinks.http]: /docs/reference/sinks/http/
[docs.sinks.humio_logs]: /docs/reference/sinks/humio_logs/
[docs.sinks.humio_metrics]: /docs/reference/sinks/humio_metrics/
[docs.sinks.influxdb_logs]: /docs/reference/sinks/influxdb_logs/
[docs.sinks.influxdb_metrics]: /docs/reference/sinks/influxdb_metrics/
[docs.sinks.kafka]: /docs/reference/sinks/kafka/
[docs.sinks.logdna]: /docs/reference/sinks/logdna/
[docs.sinks.loki]: /docs/reference/sinks/loki/
[docs.sinks.new_relic_logs]: /docs/reference/sinks/new_relic_logs/
[docs.sinks.papertrail]: /docs/reference/sinks/papertrail/
[docs.sinks.prometheus]: /docs/reference/sinks/prometheus/
[docs.sinks.pulsar]: /docs/reference/sinks/pulsar/
[docs.sinks.sematext_logs]: /docs/reference/sinks/sematext_logs/
[docs.sinks.sematext_metrics]: /docs/reference/sinks/sematext_metrics/
[docs.sinks.socket]: /docs/reference/sinks/socket/
[docs.sinks.splunk_hec]: /docs/reference/sinks/splunk_hec/
[docs.sinks.statsd]: /docs/reference/sinks/statsd/
[docs.sinks.vector]: /docs/reference/sinks/vector/
[docs.sources.apache_metrics]: /docs/reference/sources/apache_metrics/
[docs.sources.aws_kinesis_firehose]: /docs/reference/sources/aws_kinesis_firehose/
[docs.sources.docker_logs]: /docs/reference/sources/docker_logs/
[docs.sources.file]: /docs/reference/sources/file/
[docs.sources.generator]: /docs/reference/sources/generator/
[docs.sources.host_metrics]: /docs/reference/sources/host_metrics/
[docs.sources.http]: /docs/reference/sources/http/
[docs.sources.journald]: /docs/reference/sources/journald/
[docs.sources.kafka]: /docs/reference/sources/kafka/
[docs.sources.kubernetes_logs]: /docs/reference/sources/kubernetes_logs/
[docs.sources.heroku_logs]: /docs/reference/sources/heroku_logs/
[docs.sources.prometheus]: /docs/reference/sources/prometheus/
[docs.sources.socket]: /docs/reference/sources/socket/
[docs.sources.splunk_hec]: /docs/reference/sources/splunk_hec/
[docs.sources.statsd]: /docs/reference/sources/statsd/
[docs.sources.stdin]: /docs/reference/sources/stdin/
[docs.sources.syslog]: /docs/reference/sources/syslog/
[docs.sources.vector]: /docs/reference/sources/vector/
[docs.transforms.add_fields]: /docs/reference/transforms/add_fields/
[docs.transforms.add_tags]: /docs/reference/transforms/add_tags/
[docs.transforms.ansi_stripper]: /docs/reference/transforms/ansi_stripper/
[docs.transforms.aws_cloudwatch_logs_subscription_parser]: /docs/reference/transforms/aws_cloudwatch_logs_subscription_parser/
[docs.transforms.aws_ec2_metadata]: /docs/reference/transforms/aws_ec2_metadata/
[docs.transforms.coercer]: /docs/reference/transforms/coercer/
[docs.transforms.concat]: /docs/reference/transforms/concat/
[docs.transforms.dedupe]: /docs/reference/transforms/dedupe/
[docs.transforms.filter]: /docs/reference/transforms/filter/
[docs.transforms.geoip]: /docs/reference/transforms/geoip/
[docs.transforms.grok_parser]: /docs/reference/transforms/grok_parser/
[docs.transforms.json_parser]: /docs/reference/transforms/json_parser/
[docs.transforms.log_to_metric]: /docs/reference/transforms/log_to_metric/
[docs.transforms.logfmt_parser]: /docs/reference/transforms/logfmt_parser/
[docs.transforms.lua]: /docs/reference/transforms/lua/
[docs.transforms.merge]: /docs/reference/transforms/merge/
[docs.transforms.metric_to_log]: /docs/reference/transforms/metric_to_log/
[docs.transforms.reduce]: /docs/reference/transforms/reduce/
[docs.transforms.regex_parser]: /docs/reference/transforms/regex_parser/
[docs.transforms.remap]: /docs/reference/transforms/remap/
[docs.transforms.remove_fields]: /docs/reference/transforms/remove_fields/
[docs.transforms.remove_tags]: /docs/reference/transforms/remove_tags/
[docs.transforms.rename_fields]: /docs/reference/transforms/rename_fields/
[docs.transforms.sample]: /docs/reference/transforms/sample/
[docs.transforms.split]: /docs/reference/transforms/split/
[docs.transforms.route]: /docs/reference/transforms/route/
[docs.transforms.tag_cardinality_limit]: /docs/reference/transforms/tag_cardinality_limit/
[docs.transforms.tokenizer]: /docs/reference/transforms/tokenizer/
[docs.transforms.wasm]: /docs/reference/transforms/wasm/
[urls.jemalloc]: https://github.com/jemalloc/jemalloc
[urls.leveldb]: https://github.com/google/leveldb
[urls.librdkafka]: https://github.com/edenhill/librdkafka
[urls.openssl]: https://www.openssl.org/
[urls.perl_windows]: https://www.perl.org/get.html#win32
[urls.rustup]: https://rustup.rs
[urls.zlib]: https://www.zlib.net
