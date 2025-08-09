The Loki sink now filters out fields with null values when using wildcard expansion (`*`) in labels or structured metadata, preventing `<null>` strings from being sent to Loki. This matches the existing behavior of the `pair_expansion` function and ensures consistent null handling across the sink.

authors: jmealo
