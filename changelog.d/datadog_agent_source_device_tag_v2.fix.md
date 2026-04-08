`datadog_agent` source: Preserve `device` as a plain tag when decoding v2 series metrics,
instead of incorrectly prefixing it as `resource.device`. This matches the v1 series behavior
and fixes tag remapping for disk, SNMP, and other integrations that use the `device` resource type.

authors: lisaqvu
