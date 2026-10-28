Updated VRL to improve assignment performance and use platform certificate verification for `http_request`. MQTT, NATS, and GreptimeDB metrics TLS now select a crypto provider explicitly so they work when multiple Rustls crypto providers are enabled.

authors: pront
