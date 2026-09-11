Fixed event loss in the `socket` sink (and any sink using the TCP or Unix stream connectors) when
the remote peer reset or timed out the connection. Previously events were marked delivered before
the write was flushed, so a connection teardown mid-send silently dropped the in-flight event.

Events are now collected into a batch that is flushed before being finalized as delivered, and only
the events that actually flushed are marked delivered. If a flush fails, the connection is
re-established and the unflushed events are retried, giving at-least-once delivery semantics
(duplicates are possible on reconnect). The retried unit shrinks after each consecutive failure so a
peer that closes the connection at a fixed record or byte boundary still makes forward progress
instead of resending the same prefix indefinitely. The batch is bounded by both event count and
total encoded size so large records cannot pin an unbounded amount of memory while the remote is
unreachable. Reconnect attempts use exponential backoff with full jitter (capped at 5 seconds), the
socket is opened lazily only when there is data to send, and events still buffered when a sink task
is cancelled or reloaded are marked as errored so acknowledgements reflect undelivered events.

authors: simdugas
