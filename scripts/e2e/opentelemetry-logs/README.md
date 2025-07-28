# OpenTelemetry Vector E2E Log Pipeline Test

This end-to-end (E2E) test validates that log events generated in a container are correctly ingested by Vector, processed, and forwarded to an OpenTelemetry Collector sink, where they are exported to a file for verification.

## How this test works

- **Orchestrates all required services:**
  - **Log generator**: Emits fake OTLP logs.
  - **Vector**: Receives, transforms, and forwards logs to the OTEL sink and a file.
  - **OTEL Collector Source**: Forwards or processes logs upstream.
  - **OTEL Collector Sink**: Receives logs from Vector and writes them to a file.
- **Mounts volumes** to share configuration and output files between containers and the host.
- **Exposes ports** for OTLP HTTP ingestion and for accessing Vector/collector APIs if needed.

## How to Run

```shell
# from the repo root directory
./scripts/int-e2e-test.sh e2e opentelemetry-logs
```

## Notes

- The test ensures true end-to-end delivery and format compliance for OTLP logs through Vector and the OpenTelemetry Collector stack.
- Adjust the log generator, remap logic, or assertions as needed for your use case.
