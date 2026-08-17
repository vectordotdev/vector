AWS sinks (`aws_cloudwatch_logs`, `aws_s3`, `aws_kinesis_firehose`, `aws_kinesis_streams`, `aws_sns`, `aws_sqs`) now emit `component_sent_bytes_total` through the Driver instead of the transport layer, ensuring `component_id`, `component_kind`, `component_type`, and `region` labels are always present.

authors: clee2691
