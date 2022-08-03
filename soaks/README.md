# Soak Tests

This directory contains Vector's soak tests, the integrated variant of our
benchmarks. The idea was first described in [RFC
6531](../rfcs/2021-02-23-6531-performance-testing.md) and has been steadily
improved as laid out in [Issue
9515](https://github.com/vectordotdev/vector/issues/9515).

## Index of Soaks

The test definitions are in `./tests`. `ls -1 ./tests` will give you an index of
available tests. Each test has its own README.md file with more details.

## Requirements

In order to run a soak locally you will need:

* at least 4 CPUs
* docker
* python-pandas
* python-numpy
* python-tabulate

## Approach

The approach taken here is intentionally simplistic. A 'soak' is a vector
configuration and a [`lading`](https://github.com/blt/lading) configuration. The
`lading` configuration defines how load is run through Vector and it performs
all the relevant measurements of Vector, isolated in a docker container for
isolation purposes. Consider this command:

```shell
> ./soaks/soak.sh --soak datadog_agent_remap_datadog_logs --baseline 27381ec978e00553746372679c1692682328d4ec --comparison 2c9212e45758611f59e0057c93cb7e1bbca47361
```

Here we run the soak test `datadog_agent_remap_datadog_logs` comparing vector at
`27381ec978e00553746372679c1692682328d4ec` with vector at
`2c9212e45758611f59e0057c93cb7e1bbca47361`. Two containers with `lading` and a
release build of Vector will be built for each SHA. The soak itself is
combination of [`tests/datadog_agent_remap_datadog_logs/vector.toml`] and
[`tests/datadog_agent_remap_datadog_logs/lading.yaml`].

After running this command you will, in about ten minutes, see a summary:

```shell
...
2022-03-21T17:13:48.750435Z  INFO lading::captures: Recording 7 captures to comparison.captures
2022-03-21T17:13:49.749663Z  INFO lading::captures: Recording 7 captures to comparison.captures
2022-03-21T17:13:50.031762Z  INFO lading: experiment duration exceeded
2022-03-21T17:13:50.031782Z  INFO lading: waiting for 4 tasks to shutdown
2022-03-21T17:13:50.031804Z  INFO lading::target: shutdown signal received
2022-03-21T17:13:50.031811Z  INFO lading::blackhole::http: shutdown signal received
2022-03-21T17:13:50.031814Z  INFO lading::captures: shutdown signal received
2022-03-21T17:13:50.040274Z  INFO lading::generator::http: shutdown signal received
2022-03-21T17:13:51.033633Z  INFO lading: all tasks shut down
2022-03-21T17:13:51.033650Z  INFO lading: Shutting down runtime with a 10 second delay.
2022-03-21T17:13:51.034567Z  INFO lading: Bye. :)
[open_capture] reading: /tmp/soak-captures.iAn36p/datadog_agent_remap_datadog_logs/comparison/2/comparison.captures
[open_capture] reading: /tmp/soak-captures.iAn36p/datadog_agent_remap_datadog_logs/comparison/1/comparison.captures
[open_capture] reading: /tmp/soak-captures.iAn36p/datadog_agent_remap_datadog_logs/comparison/0/comparison.captures
[open_capture] reading: /tmp/soak-captures.iAn36p/datadog_agent_remap_datadog_logs/baseline/2/baseline.captures
[open_capture] reading: /tmp/soak-captures.iAn36p/datadog_agent_remap_datadog_logs/baseline/1/baseline.captures
[open_capture] reading: /tmp/soak-captures.iAn36p/datadog_agent_remap_datadog_logs/baseline/0/baseline.captures

# Soak Test Results
Baseline: 27381ec978e00553746372679c1692682328d4ec
Comparison: 2c9212e45758611f59e0057c93cb7e1bbca47361
Total Vector CPUs: 4

<details>
<summary>Explanation</summary>
<p>
A soak test is an integrated performance test for vector in a repeatable rig,
with varying configuration for vector.  What follows is a statistical summary of
a brief vector run for each configuration across SHAs given above.  The goal of
these tests are to determine, quickly, if vector performance is changed and to
what degree by a pull request. Where appropriate units are scaled per-core.
</p>

<p>
The table below, if present, lists those experiments that have experienced a
statistically significant change in their throughput performance between
baseline and comparision SHAs, with 95.0% confidence OR
have been detected as newly erratic. Negative values mean that baseline is
faster, positive comparison. Results that do not exhibit more than a
±5% change in mean throughput are discarded. An
experiment is erratic if its coefficient of variation is greater
than 0.1. The abbreviated table will be
omitted if no interesting changes are observed.
</p>
</details>

No interesting changes in throughput with confidence ≥ 95.00% and absolute Δ mean >= ±5%:


<details>
<summary>Fine details of change detection per experiment.</summary>

| experiment                       | Δ mean    |   Δ mean % | confidence   | baseline mean   | baseline stdev   | baseline stderr   |   baseline outlier % |   baseline CoV | comparison mean   | comparison stdev   | comparison stderr   |   comparison outlier % |   comparison CoV | erratic   | declared erratic   |
|----------------------------------|-----------|------------|--------------|-----------------|------------------|-------------------|----------------------|----------------|-------------------|--------------------|---------------------|------------------------|------------------|-----------|--------------------|
| datadog_agent_remap_datadog_logs | -27.87KiB |      -0.35 | 100.00%      | 7.87MiB         | 33.62KiB         | 1.81KiB           |                    0 |     0.00416805 | 7.84MiB           | 43.44KiB           | 2.34KiB             |                      0 |       0.00540507 | False     | False              |
</details>
<details>
<summary>Fine details of each soak run.</summary>

| (experiment, variant, run_id)                                                              |   total samples | mean    | std      | min     | median   | p90     | p95     | p99     | max     |   skewness |
|--------------------------------------------------------------------------------------------|-----------------|---------|----------|---------|----------|---------|---------|---------|---------|------------|
| ('datadog_agent_remap_datadog_logs', 'baseline', '8de38a1f-f99d-4cb5-9095-ecf80020ae3c')   |             115 | 7.88MiB | 31.13KiB | 7.79MiB | 7.88MiB  | 7.92MiB | 7.93MiB | 7.95MiB | 7.96MiB | -0.0422545 |
| ('datadog_agent_remap_datadog_logs', 'baseline', 'bfe5870b-b43d-418a-83f8-0edc9481af99')   |             115 | 7.86MiB | 31.02KiB | 7.79MiB | 7.86MiB  | 7.9MiB  | 7.92MiB | 7.93MiB | 7.93MiB |  0.117951  |
| ('datadog_agent_remap_datadog_logs', 'comparison', 'b88e74db-4c6d-423b-9743-b247b8d36404') |             115 | 7.86MiB | 50.02KiB | 7.74MiB | 7.85MiB  | 7.9MiB  | 7.92MiB | 8.03MiB | 8.14MiB |  2.22576   |
| ('datadog_agent_remap_datadog_logs', 'baseline', '68b6f1f3-c58e-4e57-9aba-371e500f3417')   |             116 | 7.85MiB | 32.71KiB | 7.75MiB | 7.85MiB  | 7.89MiB | 7.9MiB  | 7.92MiB | 7.92MiB | -0.221155  |
| ('datadog_agent_remap_datadog_logs', 'comparison', '5936ea23-355d-4a90-9401-95601ec76607') |             115 | 7.84MiB | 34.27KiB | 7.75MiB | 7.84MiB  | 7.89MiB | 7.89MiB | 7.9MiB  | 7.92MiB | -0.0150835 |
| ('datadog_agent_remap_datadog_logs', 'comparison', '4db00d45-b492-484a-860a-6a23326d99b4') |             115 | 7.82MiB | 36.4KiB  | 7.74MiB | 7.82MiB  | 7.86MiB | 7.88MiB | 7.94MiB | 7.95MiB |  0.598099  |
</details>
```

## Defining Your Own Soak

Assuming you can follow the pattern of an existing soak test you _should_ be
able to define a soak by copying the relevant soak into a new directory and
updating the configuration that is present in that soak's directory. Consider
the "Datadog Agent -> Remap -> Datadog Logs" soak in
[`tests/datadog_agent_remap_datadog_logs/`](tests/datadog_agent_remap_datadog_logs/). If you
`tree` that directory you'll see:

```shell
> tree tests/datadog_agent_remap_datadog_logs
tests/datadog_agent_remap_datadog_logs
├── data
│   └── .gitkeep
├── lading.yaml
├── README.md
└── vector.toml

1 directory, 4 files
```

The `data/` sub-directory is mounted into the soak container at `/data`. You can
place any static information you need here for use in a soak. The Vector
configuration `vector.toml` defines how Vector will run in the soak and the
`lading.yaml` defines how `lading` will run Vector. Please be aware that all
network communication is done via localhost. In this example the configuration
for `lading` is:

```yaml
generator:
  http:
    seed: [2, 3, 5, 7, 11, 13, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89, 97, 101, 103, 107, 109, 113, 127, 131, 137]
    headers:
      dd-api-key: "DEADBEEF"
    target_uri: "http://localhost:8282/v1/input"
    bytes_per_second: "500 Mb"
    parallel_connections: 10
    method:
      post:
        variant: "datadog_log"
        maximum_prebuild_cache_size_bytes: "256 Mb"

blackhole:
  http:
    binding_addr: "0.0.0.0:8080"
```

That is, we generate HTTP load with the format of Datadog's log format into
Vector on port 8282 at 500 Mb per second. The `lading` run here also exposes an
HTTP blackhole, a simple server that Vector can sink into that takes a minimal
time to respond. Vector is run as a sub-process and its stderr, stdout are
captured. Note that `/tmp/captures` is relative to the soak container and the
`soak.sh` will print out where on your system the captures -- including the data
`lading` collects about Vector -- can be found. The `vector.toml` is:


```toml
data_dir = "/var/lib/vector"

##
## Sources
##

[sources.internal_metrics]
type = "internal_metrics"

[sources.datadog_agent]
type = "datadog_agent"
acknowledgements = false
address = "0.0.0.0:8282"

##
## Transforms
##

[transforms.parse_message]
type = "remap"
inputs = ["datadog_agent"]
source = '''
pyld, err = parse_json(.message)
if err == null {
  .message = pyld.mineral
}
'''

##
## Sinks
##

[sinks.prometheus]
type = "prometheus_exporter"
inputs = ["internal_metrics"]
address = "0.0.0.0:9090"

[sinks.datadog_logs]
type = "datadog_logs"
inputs = ["parse_message"]
endpoint = "http://localhost:8080"
default_api_key = "DEADBEEF"
healthcheck.enabled = false
buffer.type = "memory"
buffer.max_events = 50000 # buffer 50 payloads at a time
```

Other than nothing, again, that all network communication happens over localhost
it is hoped that this is a relatively straightforward Vector configuration.

Newly added soaks in `tests/` will be ran automatically by CI.
