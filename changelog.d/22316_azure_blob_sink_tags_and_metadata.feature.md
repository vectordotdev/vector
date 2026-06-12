The `azure_blob` sink now supports a `tags` option, which sets [blob index tags](https://learn.microsoft.com/azure/storage/blobs/storage-blob-index-how-to) (`x-ms-tags`) on every created blob (parity with the `tags` option on the `aws_s3` sink).

The `azure_blob` sink now supports a `metadata` option, which sets [custom blob metadata](https://learn.microsoft.com/rest/api/storageservices/set-blob-metadata) (`x-ms-meta-*`) on every created blob (parity with the `metadata` option on the `gcp_cloud_storage` sink).

authors: danielku15
