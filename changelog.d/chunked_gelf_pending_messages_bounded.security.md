The `chunked_gelf` framing decoder now limits incomplete messages to 4096 at a time. An unauthenticated sender could previously exhaust memory by sending unique message IDs it never completed, most easily on the `socket` source in UDP mode.

`pending_messages_limit` now defaults to 4096. It was previously unset and therefore unbounded.

authors: pront
