The `aws_kinesis_streams` sink now supports optional native record aggregation.
Set `aggregation.enabled: true` and choose `aggregation.format: newline` (the
default) for text/JSONL or `aggregation.format: kpl` for standard KPL protobuf
framing. Both retain original event counts and acknowledgements. KPL compresses
each original payload independently and requires compatible stream distribution
and the `kinesis:DescribeStreamSummary` IAM permission for its startup check.
Aggregation is disabled by default.

authors: jmezger
