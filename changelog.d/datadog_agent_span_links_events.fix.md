The `datadog_agent` source now decodes Datadog span links (`spanLinks`) and span events
(`spanEvents`) into each span on the emitted trace event. Span-link `trace_id`, `trace_id_high`,
and `span_id` are 16-character lowercase hexadecimal strings so the full unsigned 64-bit range is
preserved. The `datadog_traces` sink encodes those fields back into the Agent protobuf.
`containerDebug` and `rareSamplerEnabled` are decoded from the wire but are not copied onto events.

authors: bruceg
