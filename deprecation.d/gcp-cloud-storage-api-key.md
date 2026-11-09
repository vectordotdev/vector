---
what: "Google Cloud Storage sink `api_key` configuration option"
deprecated_since: "0.62.0"
---

<a id="gcp_cloud_storage_api_key"></a>

The `api_key` configuration option for the Google Cloud Storage sink is deprecated. Google Cloud Storage does not support API keys for authentication.

Migration:

1. Remove the `api_key` option from your configuration.
2. To authenticate, use one of the methods:
    - Specify the path to a service account credentials using the `credentials_path` configuration option.
    - Set the `GOOGLE_APPLICATION_CREDENTIALS` environment variable pointing to your credentials file.
    - Rely on automatic credentials discovery when running on a GCE instance.
