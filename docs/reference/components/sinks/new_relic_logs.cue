package metadata

components: sinks: new_relic_logs: {
  title: "New Relic Logs"
  short_description: "Batches log events to [New Relic's log service][urls.new_relic] via their [log API][urls.new_relic_log_api]."
  long_description: "[New Relic][urls.new_relic] is a San Francisco, California-based technology company which develops cloud-based software to help website and application owners track the performances of their services."

  classes: {
    commonly_used: false
    function: "transmit"
    service_providers: ["New Relic"]
  }

  features: {
    batch: {
      enabled: true
      common: false,
      max_bytes: 5240000,
      max_events: null,
      timeout_secs: 1
    }
    buffer: enabled: true
    compression: {
      enabled: true
      default: null
      gzip: true
    }
    encoding: {
      enabled: true
      default: null
      json: null
      ndjson: null
      text: null
    }
    healthcheck: enabled: true
    request: {
      enabled: true
      in_flight_limit: 100,
      rate_limit_duration_secs: 1,
      rate_limit_num: 100,
      retry_initial_backoff_secs: 1,
      retry_max_duration_secs: 10,
      timeout_secs: 30
    }
    tls: enabled: false
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
    insert_key: {
      common: true
      description: "Your New Relic insert key (if applicable)."
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["xxxx","${NEW_RELIC_INSERT_KEY}"]
      }
    }
    license_key: {
      common: true
      description: "Your New Relic license key (if applicable)."
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["xxxx","${NEW_RELIC_LICENSE_KEY}"]
      }
    }
  }
}

