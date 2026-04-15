If using the `azure_logs_ingestion` sink (added in Vector 0.54.0) with Client Secret credentials, add `azure_credential_kind = "client_secret_credential"` to your sink config (this was previously the default, and now must be explicitly configured).

authors: jlaundry
