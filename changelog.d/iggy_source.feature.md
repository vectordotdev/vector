Added a new `iggy` source that consumes messages from a topic on the
[Iggy](https://iggy.apache.org) message streaming platform and emits them as
Vector log events. Each event is annotated with the originating stream, topic,
partition ID, and offset. Pulls messages either from a specific partition or
through a consumer group.

authors: jpds
