The Vector observability API has been migrated from GraphQL to gRPC for improved
performance, efficiency and maintainability. The `vector top` and `vector tap`
commands continue to work as before, as they have been updated to use the new
gRPC API internally. The gRPC service definition is available in
`proto/vector/observability.proto`.

- Remove the `api.graphql` and `api.playground` fields from your config. Vector
  will now reject configs that contain them.

- If you use `vector top` or `vector tap` with an explicit `--url`, remove the
  `/graphql` path suffix:

```bash
# Old
vector top --url http://localhost:8686/graphql

# New (the gRPC API listens at the root)
vector top --url http://localhost:8686
```

- The GraphQL API (HTTP endpoint `/graphql`, WebSocket subscriptions, and the
  GraphQL Playground at `/playground`) has been removed. You can interact with
  the new gRPC API using tools like
  [grpcurl](https://github.com/fullstorydev/grpcurl):

```bash
# Check health
grpcurl -plaintext localhost:8686 vector.observability.Observability/Health

# List components
grpcurl -plaintext localhost:8686 vector.observability.Observability/GetComponents

# Stream events (tap) — limit and interval_ms are required and must be >= 1
grpcurl -plaintext \
  -d '{"outputs_patterns": ["*"], "limit": 100, "interval_ms": 500}' \
  localhost:8686 vector.observability.Observability/StreamOutputEvents
```

authors: pront
