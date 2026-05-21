TCP-based sources that emit acknowledgements (`fluent`, `logstash`) no longer log a spurious `Error writing acknowledgement, dropping connection.` at ERROR level when the ack write fails because the peer cleanly closed its TLS session (for example, during a rolling pod restart). These graceful shutdowns now log at WARN and no longer increment `component_errors_total{error_code="ack_failed", ...}`, preventing operator dashboards/alerts from firing on routine peer disconnects. Genuine ack write failures are still logged at ERROR and continue to increment `component_errors_total`.

The `connection_shutdown_total{mode="tcp"}` counter is now incremented once per accepted source connection when it closes — regardless of cause (graceful EOF, downstream closed, decoder failure, ack write failure, shutdown signal). Previously it was not emitted by TCP sources at all.

authors: taylorchandleryoung
