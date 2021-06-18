Vector supports dynamic configuration values through a simple template syntax. If an option supports templating, it will be noted with a badge and you can use event fields to create dynamic values. For example:

```toml title="vector.toml"
[sinks.my-sink]
dynamic_option = "application={{ application_id }}"
```

In the above example, the `application_id` for each event is used to partition outgoing data.
