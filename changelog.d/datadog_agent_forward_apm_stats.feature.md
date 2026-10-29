The `datadog_agent` source now supports a `forward_apm_stats` option, which emits the APM stats sent by the Datadog Agent unmodified on a new `apm_stats` output instead of dropping them. It requires `multiple_outputs`.

authors: antoninferrand
