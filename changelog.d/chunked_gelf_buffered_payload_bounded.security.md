The `chunked_gelf` framing decoder now limits the payload buffered across incomplete messages to 128 MiB. An unauthenticated sender could previously exhaust memory by sending chunks for messages it never completed, most easily on the `socket` source in UDP mode.

`max_length` can lower the per-message ceiling. Setting it above 128 MiB raises both the per-message ceiling and the aggregate limit to that value.

authors: pront
