The `internal_metrics` source now exposes Vector's topology graph as Prometheus metrics via the `component_connections` gauge. Each connection between components is represented as a metric with labels indicating source and target component IDs, types, and kinds, enabling topology visualization and monitoring.

authors: elohmeier
