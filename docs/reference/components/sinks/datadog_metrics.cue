package metadata

components: sinks: datadog_metrics: {
  title: "Datadog Metrics"
  short_description: "Batches metric events to [Datadog's][urls.datadog] metrics service using [HTTP API](https://docs.datadoghq.com/api/?lang=bash#metrics)."
  long_description: "[Datadog][urls.datadog] is a monitoring service for cloud-scale applications, providing monitoring of servers, databases, tools, and services, through a SaaS-based data analytics platform."

  _features: {
    batch: {
      enabled: true
      common: false,
      max_events: 20,
      timeout_secs: 1
    }
    buffer: enabled: false
    checkpoint: enabled: false
    compression: enabled: false
    encoding: enabled: false
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
      timeout_secs: 60
    }
    tls: enabled: false
  }

  classes: {
    commonly_used: false
    function: "transmit"
    service_providers: ["Datadog"]
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
    api_key: {
      description: "Datadog [API key](https://docs.datadoghq.com/api/?lang=bash#authentication)"
      required: true
      warnings: []
      type: string: {
        examples: ["${DATADOG_API_KEY}","ef8d5de700e7989468166c40fc8a0ccd"]
      }
    }
    namespace: {
      common: true
      description: "A prefix that will be added to all metric names."
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["service"]
      }
    }
  }
}

