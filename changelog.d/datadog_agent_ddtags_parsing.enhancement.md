The `datadog_agent` source now contains a configuration setting `parse_ddtags`, which is disabled by default.

When enabled, the `ddtags` field (a comma separated list of key-value strings) is parsed and expanded into an
object in the event.
