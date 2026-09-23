The `internal_logs` source now preserves remote peer addresses from connection spans in the
`vector.peer_addr` field, including on errors from sources using the shared TCP connection handling.

authors: pront
