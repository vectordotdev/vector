The `gcp_pubsub` source now includes the `endpoint` label (set to the full subscription path)
on the `component_received_bytes_total` metric, consistent with other HTTP pull sources.
Previously only the `protocol` label was present.

authors: thomasqueirozb
