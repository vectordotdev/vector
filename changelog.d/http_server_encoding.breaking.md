# HTTP server `encoding` option removed {#http-server-encoding-removed}

## Summary

The deprecated `encoding` option has been removed from the `http` and
`http_server` sources. Configurations using it now fail validation.

## Migration

Replace `encoding` with `decoding` and `framing`:

| Previous `encoding` | `decoding.codec` | `framing.method` |
| --- | --- | --- |
| `text` | `bytes` | `newline_delimited` |
| `json` | `json` | `bytes` |
| `ndjson` | `json` | `newline_delimited` |
| `binary` | `bytes` | `bytes` |

authors: pront
