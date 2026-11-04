# Output field `message_key` is nullable in the Kafka source {#kafka-source-message-key-output-nullable}

## Summary

The `message_key` in output schema definition is now marked as nullable to align with runtime behavior.
While sources producing events with an absent `message_key` already emitted `null` at runtime,
the output schema definition previously declared the field as strictly non-null `bytes`.


## Migration

`N/A`


authors: Captain1653
