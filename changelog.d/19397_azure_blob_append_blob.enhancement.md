The `azure_blob` sink now supports `blob_type: append`, which writes data as Azure Append Blobs.
Unlike the default block blob mode that creates a new uniquely-named blob per batch, append mode
reuses a stable blob name and extends it on each flush — ideal for continuous log streaming
where you want a single growing file per time window.

When `blob_type` is set to `append`, `blob_append_uuid` defaults to `false` and `blob_time_format`
defaults to `%Y-%m-%dT%H` (hourly rotation), which keeps the Azure limit of 50,000 blocks per
append blob out of reach at realistic throughput. Both can still be overridden explicitly.
The Azure hard limit of 4 MiB per `append_block` call is enforced at startup via `batch.max_bytes`.

Compression is supported in append mode with `gzip`, `zstd`, or `none`. Because each batch is
compressed independently, `snappy` and `zlib` are rejected at startup: neither format can be
decoded as a concatenated sequence of streams.

Because a batch is appended to whatever the blob already holds, append mode takes the same
stream-oriented encoding defaults as the `file` sink: with `codec: json` and no explicit `framing`
it writes newline-delimited JSON, rather than the one-array-per-batch framing used for block blobs.
Explicitly configured `framing` is always used as given.

authors: danielku15
