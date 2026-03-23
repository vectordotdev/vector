Added a distribution metric socket_source_handler_duration_seconds to sources reading events
batches from a TCP, UDP or UNIX socket, which measures the time elapsed after decoding the batch and
before acknowledging it to the client (or returning to a reading state if there is no acknowledgment).

authors: gwenaskell
