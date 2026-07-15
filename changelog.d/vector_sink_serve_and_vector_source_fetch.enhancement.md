The `vector` sink now supports separate `serve` mode in which it starts up grpc server that waits
for connections and implements `pull_events` method that returns stream of events.
Also `vector` source now supports corresponding `fetch` mode that uses grpc client to call this
`pull_events` method. By default vector sink and source inherit old behaviour.

authors: Voldemat
