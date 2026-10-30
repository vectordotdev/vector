Accept valid empty OTLP exports in the OpenTelemetry source with preserved decoding,
including empty resource and scope envelopes, without emitting telemetry events.
Malformed protobuf still fails decoding and nonempty exports retain their OTLP layout.
The HTTP source also accepts OTLP JSON with native JSON receipts, retaining hexadecimal
trace/span IDs and distinguishing an empty object from an absent JSON document.

authors: rldyourmnd
