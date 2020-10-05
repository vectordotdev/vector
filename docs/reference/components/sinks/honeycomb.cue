package metadata

components: sinks: honeycomb: {
  title: "#{component.title}"
  short_description: "Batches log events to [Honeycomb][urls.honeycomb] via the [batch events API][urls.honeycomb_batch]."
  long_description: "[Honeycomb][urls.honeycomb] provides full stack observability—designed for high cardinality data and collaborative problem solving, enabling engineers to deeply understand and debug production software together."

  _features: {
    batch: {
      enabled: true
      common: false,
      max_bytes: 5242880,
      max_events: null,
      timeout_secs: 1
    }
    buffer: enabled: true
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
    service_providers: ["Honeycomb"]
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
    api_key: {
      description: "The team key that will be used to authenticate against Honeycomb."
      required: true
      warnings: []
      type: string: {
        examples: ["${HONEYCOMB_API_KEY}","some-api-key"]
      }
    }
    dataset: {
      description: "The dataset that Vector will send logs to."
      required: true
      warnings: []
      type: string: {
        examples: ["my-honeycomb-dataset"]
      }
    }
  }
}

