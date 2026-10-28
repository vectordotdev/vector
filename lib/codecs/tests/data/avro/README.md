# AVRO fixtures

This directory contains test fixture data for the avro codecs.

## Re-generating the data files

Run the following command from the repository root:

```bash
make generate-avro-fixtures
```

Or directly:

```bash
cargo run --package codecs --bin generate-avro-fixtures
```

The generator writes datum (`.avro`) and Object Container File (`.ocf.avro`) fixtures into
`lib/codecs/tests/data/avro/generated/`. The `.avsc` schema files are also written.

Note: OCF fixture files contain a randomly-generated sync marker (per the Avro spec), so the binary
content of `.ocf.avro` files changes on every regeneration. The round-trip tests always re-encode
and re-decode rather than doing byte-for-byte comparison, so this is expected.

To verify that committed fixtures match the generator output, run:

```bash
make check-avro-fixtures
```

## Known issues

Due to difference of `VrlValue` and `avro`, for example, `i32` is a type of `avro` which will be converted to `i64`, some test cases are ignored.

