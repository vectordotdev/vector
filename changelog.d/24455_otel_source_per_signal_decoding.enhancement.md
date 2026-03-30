The `opentelemetry` source now supports independent configuration of OTLP decoding for logs, metrics, and traces. This allows more granular
control over which signal types are decoded, while maintaining backward compatibility with the existing boolean configuration.

## Simple boolean form (applies to all signals)

```yaml
use_otlp_decoding: true  # All signals preserve OTLP format
# or
use_otlp_decoding: false # All signals use Vector native format (default)
```

## Per-signal configuration

```yaml
use_otlp_decoding:
  logs: false     # Convert to Vector native format
  metrics: false  # Convert to Vector native format
  traces: true    # Preserve OTLP format
```

authors: pront
