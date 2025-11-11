The `component_errors_total` metric now includes a `reason` tag when `error_code="reload"` to provide more granular information about reload
failures. Possible reasons include:

- `global_options_changed`: Reload rejected because global options (like `data_dir`) changed
- `global_diff_failed`: Reload rejected because computing global config diff failed
- `topology_build_failed`: Reload rejected because new topology failed to build/healthcheck
- `restore_failed`: Reload failed and could not restore previous config

Replaced metrics:

- `config_reload_rejected` was replaced by `component_errors_total` with `error_code="reload"` and a `reason` tag specifying the rejection type
- `config_reloaded` was replaced by the existing `reloaded_total` metric

Note: The replaced metrics were introduced in v0.50.0 but were never emitted due to a bug. These changes provide consistency across Vector's internal telemetry.

authors: pront
