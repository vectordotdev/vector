A new `azure_data_explorer` sink has been added to deliver log events to Azure Data Explorer (Kusto) via queued ingestion. The sink supports Azure Entra ID (Azure AD) service principal authentication, configurable ingestion mapping references, gzip compression, and flexible batching options. Data is uploaded as JSONL to Azure Blob Storage and then enqueued for ingestion.

authors: benmali
