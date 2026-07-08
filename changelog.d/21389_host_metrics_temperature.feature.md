The `host_metrics` source can now collect hardware temperature readings via a
new `temperature` collector. When enabled, it emits `temperature_celsius`,
`temperature_max_celsius`, and `temperature_critical_celsius` gauges, each
tagged with the `component` label of the sensor it was read from.

The collector is opt-in: add `temperature` to the `collectors` list to enable
it. Components that do not report a given value (for example a missing critical
threshold) are skipped, and environments without temperature sensors simply
produce no metrics.

authors: somaz94
