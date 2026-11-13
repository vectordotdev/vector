# Elasticsearch data-stream confinement uses the complete name

## Summary

The `elasticsearch` and `opensearch` sinks now confine the complete data-stream name instead of each
`data_stream.*` component separately. Configurations such as `type: logs`,
`dataset: system`, and `namespace: "{{ service }}"` work without disabling
confinement: destinations must start with `logs-system-`.

## Migration

For configurations with event-field templates, auto-routed destinations must also
stay within the complete template's literal prefix. Overrides of earlier static
components that change that prefix are now rejected. Prefixes on components after
the first dynamic component no longer impose separate restrictions.

Review routing overrides and any restrictions that depended on individual
component prefixes. To allow destinations outside the combined prefix, set
`dangerously_allow_unconfined_template_resolution: true` on the sink and restrict
destinations with backend permissions. Fully static configurations retain
their existing auto-routing behavior.

authors: pront
