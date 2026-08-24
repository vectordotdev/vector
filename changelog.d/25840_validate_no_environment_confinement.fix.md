`vector validate --no-environment` now catches sink confinement issues that previously
only surfaced when Vector booted.

For example, a Kafka sink with an unconfined topic template:

```yaml
sinks:
  kafka_out:
    type: kafka
    inputs: [logs]
    bootstrap_servers: "localhost:9092"
    topic: "{{ topic }}"
    encoding:
      codec: json
```

previously passed `vector validate --no-environment` and only failed with a full `vector validate` or when running the config. It now fails validation with a confinement error due to `topic` having no confinement base.

```yaml
sinks:
  kafka_out:
    type: kafka
    inputs: [logs]
    bootstrap_servers: "localhost:9092"
    topic: "events-{{ topic }}"
    encoding:
      codec: json
```

authors: thomasqueirozb
