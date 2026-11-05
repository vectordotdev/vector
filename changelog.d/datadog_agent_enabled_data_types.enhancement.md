The `datadog_agent` source now supports an `enabled_data_types` option that lists the data types (`logs`, `metrics`, `traces`, `llmobs`) the source accepts.
This replaces the (now deprecated) `disable_logs`, `disable_metrics`, `disable_traces`, and `disable_llmobs` options.
Please see the [dedicated migration notice](https://vector.dev/deprecations/#datadog_agent-disable_options).

authors: antoninferrand
