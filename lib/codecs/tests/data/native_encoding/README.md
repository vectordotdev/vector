# Native event encoding fixtures

This directory contains test fixture data for the native protobuf and JSON
codecs. These fixtures were generated when the feature was first implemented,
and we test that all the examples can be successfully parsed, parse the same
across both formats, and match the current serialized format.

In order to avoid small inherent serialization differences between JSON and
protobuf (e.g. float handling), the `generate-fixtures` feature flag in
`vector-core` activates a stricter `Arbitrary` implementation for `Event` that
produces simpler, round-trip-safe f64 values and non-empty field names. These
changes are intentionally scoped to fixture generation and not used in regular
property testing.

## Re-generating fixtures

Both this repo and the VRL repo have a `generate-fixtures` feature flag that
activates fixture-stable `Arbitrary` implementations. The vector-core
`generate-fixtures` feature automatically enables `vrl/generate-fixtures`.

### Run the generator

```bash
cargo run -p vector-core --features generate-fixtures --bin generate-fixtures
```

The binary writes files directly into this directory's `json/` and `proto/`
subdirectories, replacing the existing fixtures.
