Custom authorization strategy is now supported for sources running
HTTP servers (`http_server` source, `prometheus` source, `datadog_agent`, etc.).
If strategy is not explicitly defined, it defaults to `basic`, which is the current behavior.

authors: esensar
