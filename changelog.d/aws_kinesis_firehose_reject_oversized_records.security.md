The `aws_kinesis_firehose` source no longer falls back to forwarding raw, undecoded bytes for a record that hits the decompressed-size cap under `Compression::Auto`; such records are now rejected instead of forwarded.

authors: thomasqueirozb
