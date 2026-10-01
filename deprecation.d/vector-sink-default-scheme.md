---
what: "scheme-less `address` and `routing.endpoints` values in the `vector` sink defaulting to `http`"
deprecated_since: "0.59.0"
---

A `vector` sink endpoint such as `127.0.0.1` without an explicit scheme currently defaults to `http`.
This behavior is deprecated and will change to `https` in a future release.
Note that when TLS is enabled, a scheme-less address already defaults to `https`.

Migrate by specifying the scheme explicitly:

```yaml
sinks:
  my_sink:
    type: vector
    address: http://127.0.0.1:6000
```
