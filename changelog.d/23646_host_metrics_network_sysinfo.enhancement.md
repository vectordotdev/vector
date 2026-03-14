The `host_metrics` source network collector now uses `sysinfo` instead of the
unmaintained `heim` crate. `network_transmit_packets_total` is now emitted on
all platforms (previously linux/windows only). Windows `network_transmit_packets_drop_total`
is temporarily unavailable pending upstream sysinfo support.

authors: mushrowan
