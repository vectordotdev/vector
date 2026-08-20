# `azure_monitor_logs` sink removed {#azure-monitor-logs-sink-removed}

## Summary

The deprecated `azure_monitor_logs` sink has been removed. Configurations using it now fail
validation. Microsoft ends support for the sink's underlying Data Collector API in September
2026.

## Migration

Migrate to the `azure_logs_ingestion` sink, which uses the Azure Monitor Logs Ingestion API.
This API requires Azure Data Collection Endpoint and Data Collection Rule resources, so replace
the old workspace ID and shared key settings with the new sink's `endpoint`,
`dcr_immutable_id`, `stream_name`, and `auth` settings.

#### Old

```yaml
sinks:
  azure:
    type: azure_monitor_logs
    customer_id: "<workspace-id>"
    shared_key: "${AZURE_MONITOR_SHARED_KEY}"
    log_type: MyTable
```

#### New

```yaml
sinks:
  azure:
    type: azure_logs_ingestion
    endpoint: https://my-dce.eastus-1.ingest.monitor.azure.com
    dcr_immutable_id: dcr-000a00a000a00000a000000aa000a0aa
    stream_name: Custom-MyTable
    auth:
      azure_credential_kind: azure_cli
```

authors: pront
