Fixed a typo in a structured log field emitted by the `websocket` source: the field was previously named `protcol` and is now correctly named `protocol`. Users filtering on this trace-level field should update their queries accordingly.

authors: pront
