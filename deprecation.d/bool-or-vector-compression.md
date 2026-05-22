---
what: "Boolean syntax for the `compression` field in the `vector` sink"
deprecation_version: 0.56
---

The boolean syntax (`compression: true` / `compression: false`) is deprecated.
Use the string syntax instead: `compression: "gzip"`, `compression: "zstd"`, or `compression: "none"`.

The `bool_or_vector_compression` deserializer will be removed once the boolean syntax is no longer supported.
