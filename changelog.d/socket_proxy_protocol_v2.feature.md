Added opt-in PROXY protocol v2 support to the `socket` source in `tcp` mode via a new `proxy_protocol` option. When enabled, Vector parses and strips the v2 header prepended by a trusted upstream proxy (such as HAProxy `send-proxy-v2`), replaces the recorded peer address with the original client address, and exposes any TLV fields under the `proxy_protocol` event metadata keyed by their hex type byte (for example `proxy_protocol.tlvs."0xe0"`). This allows routing on proxy-supplied values such as a tenant identifier carried in a custom TLV.

authors: rjshrjndrn
