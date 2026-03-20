# max_depth disk buffer reproduction test

Demonstrates that `parse_json(.message, max_depth: 3)` prevents the
`InvalidProtobufPayload` disk buffer corruption caused by deeply nested JSON.

## Background

The `InvalidProtobufPayload` error occurs when `parse_json` restores deeply
nested JSON (33+ levels) into `vrl::Value` objects that exceed prost's 100-level
recursion limit during disk buffer serialization. The fix (`fd6ad3af`) added
`max_depth: 3` to bound the parsing depth.

## Test configs

| Config | `parse_json` call | Expected result with depth 33 input |
|--------|-------------------|-------------------------------------|
| `test_max_depth_sender.yaml` | `parse_json(.message, max_depth: 3)` | Succeeds — nesting bounded |
| `test_no_depth_limit_sender.yaml` | `parse_json(.message)` | Fails — `InvalidProtobufPayload` |

Both configs use `stdin` source, `remap` transform, and `blackhole` sink with
disk buffering (268 MB, block when full).

## Running the tests

Uses `depth33_nesting.json` from the parent directory (`error-reproduction/nesting/`).

```bash
# Clean up between runs
rm -rf /tmp/vector-max-depth

# Test 1: max_depth: 3 with depth 33 — should SUCCEED
cat error-reproduction/nesting/depth33_nesting.json | \
  vector --config error-reproduction/nesting/max-depth/test_max_depth_sender.yaml

# Clean up
rm -rf /tmp/vector-max-depth

# Test 2: no limit with depth 33 — should FAIL with InvalidProtobufPayload
cat error-reproduction/nesting/depth33_nesting.json | \
  vector --config error-reproduction/nesting/max-depth/test_no_depth_limit_sender.yaml

# Clean up
rm -rf /tmp/vector-max-depth
```
