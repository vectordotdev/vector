Add a `PostProcessor` trait to `SourceSender` that lets callers mutate every event on all
outputs — default and named ports — before schema metadata is attached and the event is placed
on the output channel.

authors: 20agbekodo
