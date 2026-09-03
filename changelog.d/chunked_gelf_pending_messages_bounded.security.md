The `chunked_gelf` framing decoder now limits incomplete messages to 4096 at a time. An unauthenticated sender could previously exhaust memory by sending unique message IDs it never completed, most easily on the `socket` source in UDP mode.

`pending_messages_limit` can lower this ceiling but can no longer raise it.

authors: pront
