The `chunked_gelf` framing decoder now limits incomplete messages to 4096 at a time. An unauthenticated sender could previously exhaust memory by sending unique message IDs it never completed, most easily on the `socket` source in UDP mode.

`pending_messages_limit` defaults to 4096 and can be set higher or lower.

authors: pront
