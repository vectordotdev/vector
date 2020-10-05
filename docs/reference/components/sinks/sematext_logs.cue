package metadata

components: sinks: sematext_logs: {
  title: "#{component.title}"
  short_description: "Batches log events to [Sematext][urls.sematext] via the [Elasticsearch API][urls.sematext_es]."
  long_description: "[Sematext][urls.sematext] is a hosted monitoring platform based on Elasticsearch. Providing powerful monitoring and management solutions to monitor and observe your apps in real-time."

  _features: {
    batch: {
      enabled: true
      common: false,
      max_bytes: 10490000,
      max_events: null,
      timeout_secs: 1
    }
    buffer: enabled: true
    checkpoint: enabled: false
    compression: enabled: false
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
      timeout_secs: 60
    }
    tls: enabled: false
  }

  classes: {
    commonly_used: false
    function: "transmit"
    service_providers: ["Sematext"]
  }

  statuses: {
    delivery: "at_least_once"
    development: "beta"
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
    token: {
      description: "The token that will be used to write to Sematext."
      required: true
      warnings: []
      type: string: {
        examples: ["${SEMATEXT_TOKEN}","some-sematext-token"]
      }
    }
  }
}

