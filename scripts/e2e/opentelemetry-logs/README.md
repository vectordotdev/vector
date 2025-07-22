# OpenTelemetry Vector E2E Log Pipeline Test

This end-to-end (E2E) test validates that log events generated in a container are correctly ingested by Vector, processed, and forwarded to an OpenTelemetry Collector sink, where they are exported to a file for verification.

## What the Test Does
- **Generates logs** using a custom log generator container.
- **Forwards logs** to Vector via the OpenTelemetry Protocol (OTLP).
- **Processes logs** in Vector, including optional remapping/transformation.
- **Sends logs** from Vector to an OTEL Collector sink using OTLP HTTP.
- **Exports logs** from the OTEL Collector sink to a file (`output/collector-sink.log`).
- **Test script** (`tests/e2e/opentelemetry/logs/mod.rs`) reads the exported file and asserts that expected log events are present, confirming end-to-end delivery.

## What the Docker Compose Does
- **Orchestrates all required services:**
  - **Log generator**: Emits synthetic OTLP logs at a configurable interval.
  - **Vector**: Receives, optionally transforms, and forwards logs.
  - **OTEL Collector Source**: (optional) Forwards or processes logs upstream.
  - **OTEL Collector Sink**: Receives logs from Vector and writes them to a file and/or outputs to debug.
- **Mounts volumes** to share configuration and output files between containers and the host.
- **Exposes ports** for OTLP HTTP ingestion and for accessing Vector/collector APIs if needed.

## How to Run
1. Build and start the stack:
   ```sh
   docker compose up --build
   ```
2. After logs are generated and processed, the file `output/collector-sink.log` will contain the OTLP log records exported by the collector sink.
3. Run the Rust test to assert that the expected logs are present:
   ```sh
   cargo test -p vector --test e2e -- opentelemetry::logs
   ```

## Notes
- The test ensures true end-to-end delivery and format compliance for OTLP logs through Vector and the OpenTelemetry Collector stack.
- Adjust the log generator, remap logic, or assertions as needed for your use case.
