Added a new `azure_blob` source that collects logs from blobs in Azure Blob
Storage. Discovery is event-driven: an Event Grid subscription on the storage
account delivers `Microsoft.Storage.BlobCreated` notifications to an Azure
Storage Queue, which Vector polls. Newly created blobs are downloaded,
optionally decompressed (gzip/zstd, auto-detected), decoded with any codec,
and the queue message is deleted once the events are durably accepted by the
pipeline (end-to-end acknowledgements).

Both the Event Grid and CloudEvents 1.0 notification schemas are supported and
auto-detected. Authentication reuses the same options as the `azure_blob`
sink: connection string (account key or SAS) and all Azure token credentials
(Managed Identity, Service Principal, Workload Identity, Azure CLI).

authors: Renizmy
