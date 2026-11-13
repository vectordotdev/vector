The `loki` sink with `encoding = "protobuf"` now emits a valid timestamp for events dated before the Unix epoch (1970-01-01). Previously such a timestamp was encoded with a negative nanoseconds field, which violates the `google.protobuf.Timestamp` contract (`nanos` must be in `0..=999_999_999`) and could be rejected or misread by Loki. The seconds and nanoseconds are now normalized so the nanoseconds stay non-negative.

authors: amalsaidov
