Sources running HTTP servers (`http_server` source, `prometheus` source, `datadog_agent`, etc.) now support a new `custom` authorization strategy . If a strategy is not explicitly defined, it defaults to `basic`, which is the current behavior.

authors: esensar
