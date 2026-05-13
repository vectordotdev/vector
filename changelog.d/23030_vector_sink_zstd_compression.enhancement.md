The `vector` sink now supports `zstd` compression in addition to `gzip`. This provides better
compression ratios and performance for Vector-to-Vector communication.

The compression configuration has been enhanced to support multiple algorithms while maintaining
full backward compatibility:

## Legacy boolean syntax (still supported)

```yaml
sinks:
  my_vector:
    type: vector
    address: "localhost:6000"
    compression: true   # Uses gzip (default)
    # or
    compression: false  # No compression
```

## New string syntax

```yaml
sinks:
  my_vector:
    type: vector
    address: "localhost:6000"
    compression: "zstd"  # Use zstd compression
    # Supported values: "none", "gzip", "zstd"
```

The Vector source automatically accepts both gzip and zstd compressed data, enabling seamless
communication between Vector instances using different compression algorithms.

authors: jpds
