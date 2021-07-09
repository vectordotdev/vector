This component buffers and batches data as shown in the diagram above. You'll notice that Vector treats these as sink-specific concepts rather than as global concepts. This isolates sinks, ensuring services disruptions are contained and delivery guarantees are honored.

*Batches* are flushed when 1 of 2 conditions are met:

1. The batch age meets or exceeds the configured [`timeout_secs`](#timeout_secs).
1. The batch size meets or exceeds the configured `max_size` or `max_events` (depending on which can be set in the sink).

*Buffers* are controlled using the [`buffer.*`][#buffer] options.
