Added a `GetCapabilities` RPC to the Vector observability gRPC API. Clients such as `vector top` can call this once at connection time to discover whether allocation tracing is active and the full set of metric names (counters, gauges, histograms) available on the running instance.

authors: thomasqueirozb
