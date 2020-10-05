package metadata

components: sinks: aws_kinesis_firehose: {
  title: "#{component.title}"
  short_description: "Batches log events to [Amazon Web Service's Kinesis Data Firehose][urls.aws_kinesis_firehose] via the [`PutRecordBatch` API endpoint](https://docs.aws.amazon.com/firehose/latest/APIReference/API_PutRecordBatch.html)."
  long_description: "[Amazon Kinesis Data Firehose][urls.aws_kinesis_firehose] is a fully managed service for delivering real-time streaming data to destinations such as Amazon Simple Storage Service (Amazon S3), Amazon Redshift, Amazon Elasticsearch Service (Amazon ES), and Splunk."

  _features: {
    batch: {
      enabled: true
      common: false,
      max_bytes: 4000000,
      max_events: 500,
      timeout_secs: 1
    }
    buffer: enabled: true
    checkpoint: enabled: false
    compression: {
      enabled: true
      default: "none"
      gzip: true
    }
    encoding: {
      enabled: true
      default: null
      ndjson: null
      text: null
    }
    healthcheck: enabled: true
    multiline: enabled: false
    request: {
      enabled: true
      common: false,
      in_flight_limit: 5,
      rate_limit_duration_secs: 1,
      rate_limit_num: 5,
      retry_initial_backoff_secs: 1,
      retry_max_duration_secs: 10,
      timeout_secs: 30
    }
    tls: enabled: false
  }

  classes: {
    commonly_used: false
    function: "transmit"
    service_providers: ["AWS"]
  }

  statuses: {
    delivery: "at_least_once"
    development: "stable"
  }

  support: {
      input_types: ["log"]

    platforms: {
      "aarch64-unknown-linux-gnu": true
      "aarch64-unknown-linux-musl": true
      "x86_64-apple-darwin": true
      "x86_64-pc-windows-msv": true
      "x86_64-unknown-linux-gnu": true
      "x86_64-unknown-linux-musl": true
    }

    requirements: []
    warnings: []
  }

  configuration: {
    stream_name: {
      description: "The [stream name][urls.aws_cloudwatch_logs_stream_name] of the target Kinesis Firehose delivery stream."
      required: true
      warnings: []
      type: string: {
        examples: ["my-stream"]
      }
    }
  }
}

