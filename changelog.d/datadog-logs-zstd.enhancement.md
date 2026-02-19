The Datadog Logs sink now defaults to zstd compression instead of no compression. This results in
better network efficiency and higher throughput. You can explicitly set `compression = "none"` to
restore the previous behavior of no compression, or set `compression = "gzip"` if you were previously
using gzip compression explicitly.

authors: jszwedko pront
