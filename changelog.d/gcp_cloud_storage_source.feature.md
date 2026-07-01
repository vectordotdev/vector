Added a new `gcp_cloud_storage` source that collects logs from Google Cloud Storage via Pub/Sub notifications. Objects uploaded to a GCS bucket trigger Pub/Sub notifications, which Vector polls to download and process the objects. Supports automatic decompression (gzip, zstd), multiline aggregation, and end-to-end acknowledgements.

authors: the2dl
