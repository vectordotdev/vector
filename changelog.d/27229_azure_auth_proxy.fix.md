The `azure_blob` and `azure_logs_ingestion` sinks now send Microsoft Entra ID token requests through the
configured `proxy`, so client secret, client certificate, workload identity and managed identity client
assertion authentication work where all outbound traffic must go through a proxy. Managed identity token
requests, which go to a local identity endpoint, are not sent through the configured `proxy`.

authors: anpawo
