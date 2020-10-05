package metadata

components: sinks: aws_kinesis_streams: {
  title: "AWS Kinesis Data Streams"
  short_description: "Batches log events to [Amazon Web Service's Kinesis Data Stream service][urls.aws_kinesis_streams] via the [`PutRecords` API endpoint](https://docs.aws.amazon.com/kinesis/latest/APIReference/API_PutRecords.html)."
  long_description: "[Amazon Kinesis Data Streams][urls.aws_kinesis_streams] is a scalable and durable real-time data streaming service that can continuously capture gigabytes of data per second from hundreds of thousands of sources. Making it an excellent candidate for streaming logs and metrics data."

  _features: {
    batch: {
      enabled: true
      common: false,
      max_bytes: 5000000,
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
      json: null
      ndjson: null
      text: null
    }
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
    partition_key_field: {
      common: true
      description: "The log field used as the Kinesis record's partition key value."
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["user_id"]
      }
    }
    stream_name: {
      description: "The [stream name][urls.aws_cloudwatch_logs_stream_name] of the target Kinesis Logs stream."
      required: true
      warnings: []
      type: string: {
        examples: ["my-stream"]
      }
    }
  }
}

