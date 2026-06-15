The `azure_blob` sink now supports `blob_type: append`, which writes data as Azure Append Blobs.
Unlike the default block blob mode that creates a new uniquely-named blob per batch, append mode
reuses a stable blob name and extends it on each flush — ideal for continuous log streaming
where you want a single growing file per time window.

When `blob_type` is set to `append`, `blob_append_uuid` defaults to `false` and `blob_time_format`
defaults to `%Y-%m-%d` (daily rotation). Both can still be overridden explicitly.
The Azure hard limit of 4 MiB per `append_block` call is enforced at startup via `batch.max_bytes`.
