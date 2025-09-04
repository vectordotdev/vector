Added support for varint length delimited framing for protobuf, which is compatible with standard protobuf streaming implementations and tools like ClickHouse.

Users can now opt-in to varint framing by explicitly specifying `framing.method: varint_length_delimited` in their configuration. The default remains length-delimited framing for backward compatibility.

authors: modev2301
