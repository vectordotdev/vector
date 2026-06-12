Add a `PostProcessor` hook to `SourceSender` that allows a hard-coded Rust closure to be applied
to every event emitted by a source, immediately after schema metadata is attached and before the
event is placed on the output channel.

authors: 20agbekodo
