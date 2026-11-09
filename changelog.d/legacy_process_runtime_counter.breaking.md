# Legacy `process_runtime` counter removed {#legacy-process-runtime-counter-removed}

## Summary

The `host_metrics` source no longer emits the `process_runtime` counter. It was deprecated in
0.59.0 in favor of `process_runtime_total`, which reports the same value with the same tags.

## Migration

Update dashboards and alerts that reference `process_runtime` to use `process_runtime_total`.

authors: JinchengCao
