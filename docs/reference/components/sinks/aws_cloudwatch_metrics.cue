package metadata

components: sinks: aws_cloudwatch_metrics: {
  title: "#{component.title}"
  short_description: "Streams metric events to [Amazon Web Service's CloudWatch Metrics service][urls.aws_cloudwatch_metrics] via the [`PutMetricData` API endpoint](https://docs.aws.amazon.com/AmazonCloudWatch/latest/APIReference/API_PutMetricData.html)."
  long_description: "[Amazon CloudWatch][urls.aws_cloudwatch] is a monitoring and management service that provides data and actionable insights for AWS, hybrid, and on-premises applications and infrastructure resources. With CloudWatch, you can collect and access all your performance and operational data in the form of logs and metrics from a single platform."

  _features: {
    batch: {
      enabled: true
      common: false,
      max_events: 20,
      timeout_secs: 1
    }
    buffer: enabled: false
    checkpoint: enabled: false
    compression: {
      enabled: true
      default: "none"
      gzip: true
    }
    encoding: enabled: false
    healthcheck: enabled: true
    multiline: enabled: false
    request: enabled: false
    tls: enabled: false
  }

  classes: {
    commonly_used: false
    function: "transmit"
    service_providers: ["AWS"]
  }

  statuses: {
    delivery: "at_least_once"
    development: "beta"
  }

  support: {
      input_types: ["metric"]

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
    namespace: {
      common: true
      description: "A [namespace](https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/cloudwatch_concepts.html#Namespace) that will isolate different metrics from each other."
      required: true
      warnings: []
      type: string: {
        examples: ["service"]
      }
    }
  }
}
