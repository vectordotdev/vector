The `mqtt` sink now closes its broker connection after the sink stops, for example when a configuration reload removes or replaces it. Before, the connection stayed open after the sink stopped. A replacement sink that uses the same client ID could then be repeatedly disconnected by the broker.

The sink drains queued messages and QoS exchanges as part of its shutdown. A connection error ends the drain without failing a reload or reconnecting a stopped sink.

Vector's `--graceful-shutdown-limit-secs` or `--no-graceful-shutdown-limit` setting now also controls how long old sinks can drain during a configuration reload. All affected sink tasks share one reload deadline. This applies to all sinks, not only MQTT.

The MQTT source and sink now use `rumqttc-v4-next` 0.34.0 and retain MQTT 3.1.1 support.

authors: thomasqueirozb
