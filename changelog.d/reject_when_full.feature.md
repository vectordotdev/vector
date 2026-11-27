Added a new `reject` value for a sink buffer's `when_full` option. Like `drop_newest`, it drops the
newest event when the buffer is full, but it acknowledges the dropped event as *errored* rather than
as a successful delivery. This lets a source with `acknowledgements` enabled signal the failure to
its client instead of silently reporting success for data that was discarded. For example, the
`http_server` source returns `500 Internal Server Error` rather than its configured success code (by
default `200 OK`) when the downstream sink's buffer is full and rejecting events. As buffer pressure
is transient, clients should treat this as a retriable error and back off.

`reject` applies to both `disk` and `memory` buffers. It differs from `drop_newest`, which is
unchanged and remains best-effort, acknowledging dropped events as delivered.

authors: PradeepSundarajan
