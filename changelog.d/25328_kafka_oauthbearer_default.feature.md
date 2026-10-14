Added support for `sasl.oauthbearer.method=default` in Kafka sink and source. When
`sasl.oauthbearer.token.endpoint.url` is set in `librdkafka_options` and the method is
not `oidc`, Vector now implements the OAUTHBEARER token refresh callback, POSTing to the
configured endpoint per RFC 6749 §4.4 and reading `access_token` and `expires_in` from
the response. This enables OAuth2 providers that return opaque tokens, non-standard JWT
expiry fields, or custom grant types (e.g. `authorization_code`) that are incompatible
with `method=oidc`.

authors: dvilaverde
