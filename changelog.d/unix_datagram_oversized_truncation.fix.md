Unix datagram sources (the `socket` source in `unix_datagram` mode and the `statsd` source with
`unix_mode: datagram`) no longer emit a corrupted final event when a datagram exceeds the receive
size limit. The limit is `max_length` plus one byte for a trailing delimiter, where `max_length` is
the framing's `max_length` for `newline_delimited`, `character_delimited` and `octet_counting`
framing and 102400 bytes otherwise. The complete frames of such a datagram are still decoded, but
its incomplete final frame is now discarded and the datagram is reported as an `oversized_datagram`
error. A datagram without any complete frame is still passed truncated to the framing, as before:
framing with a `max_length` applies its `oversized_action`, the `statsd` source drops it, and the
default `bytes` framing still emits it truncated.

authors: FStelzer
