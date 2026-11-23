End-to-end acknowledgements now report delivery failure when an event is dropped
because routing or encoding failed in Vector's shared sink paths. For example,
if an HTTP sink's URI contains `{{ route }}` and an incoming event lacks the
`route` field, an upstream `http_server` source with acknowledgements enabled
returns HTTP `400 Bad Request` to the sender. Previously the sender could receive
a success response for that dropped event.

authors: pront
