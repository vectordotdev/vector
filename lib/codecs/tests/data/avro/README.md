# AVRO fixtures

This directory contains test fixture data for the avro codecs.

# re-generate the data files
There is currently a multi-step procedure to re-generate the data files.

1. apply `avro_generate_fixtures.patch`
2. run the bash

```bash
    $ mkdir lib/codecs/_avro
    $ cargo test --package codecs --test avro_generate_test_case --  --exact --nocapture
```

That test case writes out the appropriate files into the dirs, which then need to be
moved to their location here.

# Known issues
Due to difference of `VrlValue` and `avro`, for example, `i32` is a type of `avro` which will be converted to `i64`, some test cases are ignored.