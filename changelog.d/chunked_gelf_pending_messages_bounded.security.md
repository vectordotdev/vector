The `chunked_gelf` framing decoder now bounds what it buffers for incomplete messages, to at most 4096 messages and 128 MiB of payload at a time. An unauthenticated sender could previously exhaust memory by sending chunk headers for messages it never completed, most easily on the `socket` source in UDP mode.

Please note that `pending_messages_limit` and `max_length` can now only lower these bounds, not raise them, so a configuration setting `max_length` above 128 MiB will start dropping larger chunked messages.

authors: pront
