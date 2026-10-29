The `datadog_traces` sink now supports a `disable_apm_stats` option, which stops the sink from computing APM stats from the traces it receives, for example when the Datadog Agent's stats are forwarded separately.

authors: antoninferrand
