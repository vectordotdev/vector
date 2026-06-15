Add a `PostProcessor` hook to `SourceSender` that runs a hard-coded Rust closure on every event
immediately after schema metadata is attached and before the event is placed on the output channel.
Also fixes `buffer_send_duration_seconds` telemetry to exclude post-processor CPU time by capturing
the send reference immediately before the channel enqueue.

authors: 20agbekodo
