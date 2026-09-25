The `dnstap` source now correctly reports the client address when a producer sends an IPv4-mapped IPv6 address (`::ffff:a.b.c.d`) while labeling the socket family as `INET`. CoreDNS (via Go's `net.IP`) does this in practice, which previously caused `sourceAddress` (and `responseAddress`) to be parsed as `0.0.0.0` because only the leading four zero bytes were read. Such addresses are now unwrapped to the real IPv4 address.

authors: xfocus3
