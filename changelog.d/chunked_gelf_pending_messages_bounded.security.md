The `chunked_gelf` framing decoder now applies a configurable limit to the number of incomplete messages held in memory. An unauthenticated sender could previously exhaust memory by sending unique message IDs it never completed, most easily on the `socket` source in UDP mode.

`pending_messages_limit` now defaults to 4096. It was previously unset and therefore unbounded.

authors: pront
