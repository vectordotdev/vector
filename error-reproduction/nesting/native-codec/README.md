# Native codec reproduction test

Demonstrates the prost recursion limit on the gRPC path using Vector's native
(protobuf) codec. The `type: vector` sink and source implicitly use the native
codec — you don't configure it, it's built in.

## Comparison with other test directories

| Directory | Path tested | Error when depth > 32 |
|-----------|------------|-----------------------|
| `disk/` | stdin → disk buffer → blackhole | `InvalidProtobufPayload` |
| `source-sink/` | stdin → vector sink → vector source | gRPC `Status::Internal("...recursion limit reached")` |
| `native-codec/` | stdin → vector sink → vector source | same as source-sink |
| `max-depth/` | stdin → parse_json(max_depth:3) → disk buffer | **No error** (nesting bounded) |

All errors come from the same root cause: `prost::DecodeError("recursion limit reached")`.
The error message differs because each path wraps the prost error differently:

| Path | Encode | Decode | Error type |
|------|--------|--------|------------|
| Disk buffer | `ser.rs:98` | `ser.rs:108-113` | `DecodeError::InvalidProtobufPayload` (wrapped) |
| gRPC (type: vector) | tonic + prost | tonic + prost | gRPC `Status::Internal` |

## Why not a file-based native codec test?

The native codec can be specified explicitly as `encoding.codec: native` on sinks
like Kafka, Pulsar, or socket. However, Vector's `file` source does not support a
`decoding` option — it's a line-oriented reader. So there's no way to read back
native-encoded files within Vector. The gRPC test (type: vector source/sink) is the
only way to test the native codec end-to-end without an external service like Kafka.

## Running the test

```bash
# Terminal 1: start receiver
vector --config error-reproduction/nesting/native-codec/test_native_receiver.yaml

# Terminal 2: send depth 32 — should succeed, receiver prints JSON to stdout
cat error-reproduction/nesting/depth32_nesting.json | \
  vector --config error-reproduction/nesting/native-codec/test_native_sender.yaml

# Terminal 2: send depth 33 — should fail
cat error-reproduction/nesting/depth33_nesting.json | \
  vector --config error-reproduction/nesting/native-codec/test_native_sender.yaml

# Expected error on sender (WARN, retries until shutdown):
#   WARN sink{component_type=vector}:request: vector::sinks::util::retries:
#   Retrying after error. error=Request failed: status: Internal,
#   message: "failed to decode Protobuf message: ...
#   Value.kind: ValueMap.fields: Value.kind: ... recursion limit reached"

# Clean up
rm -rf /tmp/vector-native-sender /tmp/vector-native-receiver
```
