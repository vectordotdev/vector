Fixed event loss in the `socket` sink (and any sink using the TCP or Unix stream connectors) when
the remote peer reset or timed out the connection. Previously events were marked delivered before
the write was flushed, so a connection teardown mid-send silently dropped the in-flight event.

Events are now collected into a batch that is flushed as a unit and only finalized as delivered
once the flush succeeds. If the flush fails, the connection is re-established and the same batch is
retried, giving at-least-once delivery semantics (duplicates are possible on reconnect). Reconnect
attempts use exponential backoff with full jitter (capped at 5 seconds), the socket is opened lazily
only when there is data to send, and events still buffered when a sink task is cancelled or reloaded
are marked as errored so acknowledgements reflect undelivered events.

authors: simdugas
