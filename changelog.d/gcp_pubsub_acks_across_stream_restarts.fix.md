The `gcp_pubsub` source no longer drops acknowledgements when a streaming pull ends. Batches
that are finalized after the stream that delivered them was closed by the server, failed, or was
restarted for a new authentication token are now acknowledged on the next stream, including while
that stream waits for its first messages, instead of being redelivered once their ack deadline
expires. Shutdown now also waits for pending batches while a stream is starting, and messages
that the source fails to emit have their leases released for immediate redelivery.

authors: mtenpow
