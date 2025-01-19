The `socket` source with `udp` mode now supports joining multicast groups via the `multicast_groups` option
of that source. This allows the source to receive multicast packets from the specified multicast groups.

Note that in order to work properly, the `socket` address must be set to `0.0.0.0` and not
to `127.0.0.1` (localhost) or any other specific IP address. If other IP address is used, the host's interface
will filter out the multicast packets as the packet target IP (multicast) would not match the host's interface IP.

authors: jorgehermo9
