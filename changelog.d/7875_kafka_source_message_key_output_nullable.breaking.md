# Output field `message_key` is nullable in the Kafka source {#kafka-source-message-key-output-nullable}

## Summary

The `message_key` field in the output schema definition is now marked as nullable to align with runtime behavior.
While sources producing events with an absent `message_key` already emitted `null` at runtime,
the output schema definition previously declared the field as strictly non-null `bytes`.

Configurations with `schema.enabled: true` that reference `message_key` (`.message_key` in the legacy log namespace, or `%kafka.message_key` in the Vector log namespace)
in downstream VRL transforms without handling `null` will now fail VRL compilation.

## Migration

Migrate by replacing:

```vrl
.upper_key = upcase(.message_key)
```

with:

```vrl
# Update the expression to be infallible by adding a `!`
.upper_key = upcase!(.message_key)

# Or handle fallibility explicitly
.upper_key, err = upcase(.message_key)
```

authors: Captain1653
