Enable Kafka OIDC (OAuthBearer) by adding an opt-in `rdkafka-curl-dynamic` feature to link curl dynamically. The default static-curl build remains unchanged.

This affects both the Kafka source and sink; no configuration changes are required unless you choose to build with the new feature.
