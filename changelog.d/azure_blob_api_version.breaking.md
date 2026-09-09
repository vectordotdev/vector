# Azure Blob Shared Key API version uses the SDK default {#azure-blob-shared-key-api-version}

## Summary

The Azure Blob sink now supports selecting the Azure Storage service API version through the
`api_version` option. When this option is unset, Shared Key authentication now uses the version
selected by the Azure SDK instead of the previously hardcoded `2025-11-05` version. Other
authentication methods already used the SDK-selected version. This change can affect Azure Stack
Hub and other compatible storage services that do not support the SDK's default API version.

## Migration

Set `api_version` to a version supported by the storage service. To preserve the previous Shared
Key behavior, add the following to the existing Azure Blob sink configuration:

```yaml
sinks:
  my_azure_blob_sink:
    api_version: "2025-11-05"
```

authors: opencow
