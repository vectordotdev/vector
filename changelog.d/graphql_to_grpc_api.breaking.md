The Vector observability API has been migrated from GraphQL to gRPC for improved performance and efficiency.

**Breaking Change:** The GraphQL Playground that was previously available at `http://<vector_api_address>/playground`
has been removed. Users who were accessing Vector's observability API through the GraphQL Playground should now
use gRPC tools instead.

**Replacement:** You can interact with the new gRPC API using tools like:

- [`grpcurl`](https://github.com/fullstorydev/grpcurl) - command-line tool for gRPC
- [BloomRPC](https://github.com/bloomrpc/bloomrpc) - GUI client for gRPC
- [Postman](https://www.postman.com/) - supports gRPC requests

The `vector top` and `vector tap` commands continue to work as before, as they have been updated to use the
new gRPC API internally. The gRPC service definition is available in `proto/vector/observability.proto`.

**Example using grpcurl:**

```bash
# Check health
grpcurl -plaintext localhost:8686 vector.observability.Observability/Health

# List components
grpcurl -plaintext localhost:8686 vector.observability.Observability/GetComponents

# Stream events (tap)
grpcurl -plaintext -d '{"outputs_patterns": ["*"], "limit": 1000, "interval_ms": 100}' \
  localhost:8686 vector.observability.Observability/StreamOutputEvents
```

authors: pront
