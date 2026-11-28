The `internal_metrics` source now emits a `component_connections` gauge for every connection between components in the running topology, tagged with the IDs, types, and kinds of both components and the upstream output name. The series follow configuration reloads, so the topology can be visualized and joined with per-component metrics, for example in a Grafana node graph.

authors: elohmeier
