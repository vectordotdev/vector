The `nats` source can now expose NATS message headers on each event. Set the new `headers_key` option to the field where headers should be written (for example `headers_key: headers`). Each header name maps to an array of its string values, since NATS headers can be multi-valued. By default `headers_key` is unset and no headers are exposed, preserving backwards compatibility.

authors: Simon Dugas
