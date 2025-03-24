This change introduces a new configuration option in the StatsD source: `convert_timers_to_seconds`. By default, timing values in milliseconds (`ms`) are converted to seconds (`s`), preserving the current behavior. With this enhancement, users can disable the conversion by setting the option to `false`, which retains the original millisecond values. This flexibility is particularly useful for those using downstream systems (such as Datadog or Grafana) that expect timer metrics in their original units.

authors: devkoriel
