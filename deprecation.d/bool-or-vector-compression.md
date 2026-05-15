---
what: "Boolean syntax for the `compression` field in the `vector` sink"
announcement_version: "0.57.0"
deprecation_version: "0.57.0"
---

The boolean syntax (`compression: true` / `compression: false`) is deprecated.
Use the string syntax instead: `compression: "gzip"`, `compression: "zstd"`, or `compression: "none"`.

The `bool_or_vector_compression` deserializer will be removed once the boolean syntax is no longer supported.
