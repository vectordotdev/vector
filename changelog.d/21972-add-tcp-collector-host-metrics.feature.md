The `network` collector in the `host_metrics` source has been
updated to expose three metrics about the TCP stack of the system:
* `network_tcp_connections_total`: The total number of TCP
  connections. It includes the `state` of the connection as a tag.
* `network_tcp_tx_queued_bytes_total`: The sum of the number of bytes
   in the send queue across all connections.
* `network_tcp_rx_queued_bytes_total`: The sum of the number of bytes
  in the receive queue across all connections.

These metrics are only available for Linux systems.

authors: aryan9600
