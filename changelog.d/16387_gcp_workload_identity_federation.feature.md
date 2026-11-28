GCP components (`gcp_cloud_storage`, `gcp_pubsub`, `gcp_stackdriver_logs`, `gcp_stackdriver_metrics`, `gcp_chronicle_unstructured`) now accept [Workload Identity Federation](https://cloud.google.com/iam/docs/workload-identity-federation) credentials files (`"type": "external_account"`) through `credentials_path` or the `GOOGLE_APPLICATION_CREDENTIALS` environment variable. This lets Vector running outside of GCP authenticate with short-lived federated tokens instead of long-lived service account keys.

authors: jpatra72
