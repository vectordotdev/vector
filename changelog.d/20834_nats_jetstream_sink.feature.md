Add possibility to use NATS JetStream in NATS sink. Can be turned on/off via `jetstream` option (default is false).

### Example

#### Config

```
sinks:
  nats:
    type: nats
    inputs:
      - in
    subject: nork
    url: "nats://localhost:4222"
    jetstream: true
    encoding:
      codec: json
```