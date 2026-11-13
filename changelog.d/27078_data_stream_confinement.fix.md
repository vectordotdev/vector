The `elasticsearch` and `opensearch` sinks now accept data-stream components without
their own literal prefix when the complete data-stream name supplies a confinement
boundary. For example, `type: logs`, `dataset: system`, and
`namespace: "{{ service }}"` use `logs-system-` as the boundary, including for
auto-routed destinations. Existing component prefix restrictions and routing
behavior for previously supported configurations are preserved.

authors: pront
