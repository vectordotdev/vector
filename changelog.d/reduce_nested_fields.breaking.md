Reduce transforms can now properly aggregate nested fields.
This is a breaking change because merging object elements were using the "discard" strategy
but the new behavior is using the default strategy based on the element type.
