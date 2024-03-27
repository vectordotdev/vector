Previously the `datadog_agent` setting `parse_ddtags` parsed the tag string into an Object. It is now parsed into an Array of `key:value` strings, which matches the  behavior of the Datadog logs backend intake.

Additionally, the `datadog_logs` sink was not re-constructing the tags into the format that Datadog intake expects. The sink log encoding was fixed to re-assemble the tags into a unified string.
