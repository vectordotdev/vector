---
what: "Configuring a PKCS#12 archive via `tls.key_file` in `amqp` source and sink"
deprecated_since: "0.62.0"
---

Configuring a PKCS#12 archive via `tls.key_file` in the `amqp` source and sink is deprecated.
Pass the archive via `tls.crt_file` instead.

Migrate by replacing:

```yaml
sinks: # or sources
  my_amqp:
    type: amqp
    tls:
      key_file: "/path/to/identity.p12"
```

with:

```yaml
sinks: # or sources
  my_amqp:
    type: amqp
    tls:
      crt_file: "/path/to/identity.p12"
```
