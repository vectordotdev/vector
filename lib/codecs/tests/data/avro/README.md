# AVRO fixtures

This directory contains test fixture data for the avro codecs.

## re-generate the data files

There is currently a multi-step procedure to re-generate the data files.

1. run the bash

```bash
cargo run --package codecs --bin generate-avro-fixtures
```

That test case writes out the appropriate files into `lib/codecs/tests/data/avro/generated/` dir.

## Known issues

Due to difference of `VrlValue` and `avro`, for example, `i32` is a type of `avro` which will be converted to `i64`, some test cases are ignored.

