The `prometheus_exporter` sink now sends `Content-Length` on uncompressed responses instead of `Transfer-Encoding: chunked`. It also sends `Vary: Accept-Encoding` on responses that can be compressed, and it compresses responses for clients that send `Accept-Encoding: x-gzip`. Status codes and response bodies are unchanged.

authors: thomasqueirozb
