The `codecs-parquet` Cargo feature is now enabled in all official release artifacts (Linux GNU/musl on x86_64/aarch64/armv7/arm, macOS, and Windows). Previously the precompiled binaries rejected the AWS S3 sink's `batch_encoding` field with `unknown field 'batch_encoding'`, even though the field is documented and the feature was advertised in the v0.55.0 release notes. Users no longer need to compile Vector from source to use Parquet encoding in the AWS S3 sink.

authors: prontidis
