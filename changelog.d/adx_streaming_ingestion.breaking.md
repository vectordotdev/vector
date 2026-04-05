The `azure_data_explorer` sink no longer uses **queued ingestion** (Azure Blob upload plus queue notification). It now sends data only via **streaming ingestion**: `POST {ingestion_endpoint}/v1/rest/ingest/{database}/{table}` with `streamFormat=MultiJSON` and optional `mappingName`, using the same Entra ID application (client ID + secret) credentials as before.

**What you need to do when upgrading**

- Enable [streaming ingestion](https://learn.microsoft.com/en-us/azure/data-explorer/ingest-data-streaming) on your Azure Data Explorer cluster and define a [streaming ingestion policy](https://learn.microsoft.com/en-us/kusto/management/streaming-ingestion-policy) on the target database or table.
- Expect stricter request size limits than with queued ingestion (Vector defaults to smaller batches to stay under the roughly 4 MiB streaming limit per request).
- If Azure rejects ingest for `MultiJSON`, configure `mapping_reference` to match a pre-created JSON ingestion mapping on the table, as required by the streaming ingest API for mapped JSON payloads.
