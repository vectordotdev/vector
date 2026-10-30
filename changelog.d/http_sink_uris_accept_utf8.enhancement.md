HTTP-based sinks now accept UTF-8 characters in the path or query of their endpoint URI, for example `https://example.com/café?tag=é`. Before, such an endpoint failed with `invalid uri character`. Hosts must still be ASCII, so use the punycode form of an internationalized domain name.

This does not apply to the `http` sink's `uri`, the `opentelemetry` sink's `protocol.uri`, or the `axiom` sink's `url`. Vector treats these as templates, and they must still contain only ASCII characters, both as written in the config and after event fields are filled in.

authors: thomasqueirozb
