The `datadog_agent` source now accepts `apm_stats` (alpha) in `enabled_data_types`, which emits the APM stats sent by the Datadog Agent unmodified on a new `apm_stats` output instead of dropping them. It requires `multiple_outputs` and is not enabled when `enabled_data_types` is omitted.

authors: antoninferrand
