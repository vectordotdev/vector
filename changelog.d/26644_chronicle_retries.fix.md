The `gcp_chronicle_unstructured` sink now retries transient HTTP and connection failures according to its `request.retry_*` settings. Previously, it dropped affected batches immediately, even with a disk buffer.

authors: DeviousCardi
