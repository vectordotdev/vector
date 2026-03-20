# Disk buffer case

```bash
# This works always
cat depth32_nesting.json | vector --config disk/test_recursion_sender.yaml

cat depth33_nesting.json | vector --config disk/test_recursion_sender.yaml
# -->
# 2026-03-10T18:15:57.170671Z ERROR sink{component_kind="sink" component_id=blackhole
# component_type=blackhole}: vector_buffers::internal_events: Error encountered during
# buffer read. error=failed to decoded record: InvalidProtobufPayload
# error_code="decode_failed" error_type="reader_failed" stage="processing"


# on subsequent runs of either:
cat depth33_nesting.json | vector --config disk/test_recursion_sender.yaml
# or
cat depth32_nesting.json | vector --config disk/test_recursion_sender.yaml
# -->
# 2026-03-10T18:16:01.506210Z ERROR vector::topology::builder: Configuration error.
# error=Sink "blackhole": error occurred when building buffer: failed to build individual
# stage 0: failed to seek to position where writer left off: failed to validate the last
# written record: failed to decoded record: InvalidProtobufPayload
# internal_log_rate_limit=false

# reset using
rm -rf /tmp/vector
```


# Vector source-sink case

```bash

# In terminal 1:

vector --config source-sink/test_recursion_receiver.yaml

# In terminal 2:

# This works always
cat depth32_nesting.json| vector --config source-sink/test_recursion_sender.yaml

cat depth33_nesting.json | vector --config disk/test_recursion_sender.yaml
# -->
# 2026-03-10T18:24:30.268256Z  WARN sink{component_kind="sink" component_id=sender
# component_type=vector}:request{request_id=1}: vector::sinks::util::retries: Retrying
# after error. error=Request failed: status: Internal, message: "failed to decode Protobuf
# message: Value.kind: ValueMap.fields: Value.kind: ValueMap.fields: Value.kind: ValueMap.
# fields: Value.kind: ValueMap.fields: Value.kind: ValueMap.fields: Value.kind: ValueMap.
# fields: Value.kind: ValueMap.fields: Value.kind: ValueMap.fields: Value.kind: ValueMap.
# fields: Value.kind: ValueMap.fields: Value.kind: ValueMap.fields: Value.kind: ValueMap.
# fields: Value.kind: ValueMap.fields: Value.kind: ValueMap.fields: Value.kind: ValueMap.
# fields: Value.kind: ValueMap.fields: Value.kind: ValueMap.fields: Value.kind: ValueMap.
# fields: Value.kind: ValueMap.fields: Value.kind: ValueMap.fields: Value.kind: ValueMap.
# fields: Value.kind: ValueMap.fields: Value.kind: ValueMap.fields: Value.kind: ValueMap.
# fields: Value.kind: ValueMap.fields: Value.kind: ValueMap.fields: Value.kind: ValueMap.
# fields: Value.kind: ValueMap.fields: Value.kind: ValueMap.fields: Value.kind: ValueMap.
# fields: Value.kind: ValueMap.fields: Value.kind: ValueMap.fields: Value.kind: Log.
# fields: EventWrapper.event: PushEventsRequest.events: recursion limit reached", details:
# [], metadata: MetadataMap { headers: {"content-type": "application/grpc", "date": "Tue,
# 10 Mar 2026 18:24:30 GMT", "content-length": "0"} }


# 2026-03-10T18:24:20.946874Z  WARN sink{component_kind="sink" component_id=sender
# component_type=vector}:request{request_id=1}: vector::sinks::util::retries: Internal log
# [Retrying after error.] is being suppressed to avoid flooding.

# ^ above two warnings repeat until shutdown after 1 minute.

# 2026-03-10T18:25:20.143568Z ERROR vector::topology::running: components="sender" Failed
# to gracefully shut down in time. Killing components. internal_log_rate_limit=false

# This still works
cat depth32_nesting.json| vector --config source-sink/test_recursion_sender.yaml

```
