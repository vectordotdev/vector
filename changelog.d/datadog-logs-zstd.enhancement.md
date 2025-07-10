The Datadog Logs sink now defaults to zstd compression instead of gzip. This results in less
resource and higher throughput. You can explicitly set the compression to `gzip` to restore the
previous behavior.

authors: jszwedko
