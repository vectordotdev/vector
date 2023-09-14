# Workload Checks

The `smp` tool performs a nightly run of 'checks' to determine if Vector is fit for purpose.
The 'checks' can help us answer questions about CPU usage, memory consumption, throughput etc.
By consistently running these checks we establish a historical dataset [here](https://app.datadoghq.com/dashboard/wj9-9ds-q49?refresh_mode=sliding&from_ts=1694089061369&to_ts=1694693861369&live=true).

## Adding an Experiment

You can read more about the workload requirements [here](https://github.com/DataDog/datadog-agent/blob/main/test/workload-checks/README.md).
