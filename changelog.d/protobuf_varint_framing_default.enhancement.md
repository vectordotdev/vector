Protobuf encoding now uses varint length delimited framing by default, which is compatible with standard protobuf streaming implementations and tools like ClickHouse.

This change improves compatibility with the protobuf streaming specification and reduces the need for explicit framing configuration in most cases.

authors: modev2301 