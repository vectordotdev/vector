# Fluent -> Elasticsearch

This soak tests fluent source feeding directly into elasticsearch sink.

## Method

Lading `tcp_gen` is used to generate log load into vector, `http_blackhole`
acts as an Elasticsearch.
