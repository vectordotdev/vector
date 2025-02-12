The `host_metrics` source has a new collector, `tcp`. The `tcp`
collector exposes three metrics related to the TCP stack of the
system:

* `tcp_connections_total`: The total number of TCP connections. It
  includes the `state` of the connection as a tag.
* `tcp_tx_queued_bytes_total`: The sum of the number of bytes in the
   send queue across all connections.
* `tcp_rx_queued_bytes_total`: The sum of the number of bytes in the
  receive queue across all connections.

This collector is enabled only on Linux systems.

authors: aryan9600
