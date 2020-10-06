package metadata

components: sinks: papertrail: {
  title: "Papertrail"
  short_description: "Streams log events to [Papertrail][urls.papertrail] via [Syslog][urls.papertrail_syslog]."
  long_description: "[Papertrail][urls.papertrail] is a web-based log aggregation application used by developers and IT team to search and view logs in real time."

  classes: {
    commonly_used: false
    function: "transmit"
    service_providers: ["Papertrail"]
  }

  features: {
    batch: enabled: false
    buffer: enabled: true
    compression: enabled: false
    encoding: {
      enabled: true
      default: null
      json: null
      ndjson: null
      text: null
    }
    healthcheck: enabled: true
    request: enabled: false
    tls: {
      enabled: true
      can_enable: true
      can_verify_certificate: true
      can_verify_hostname: true
      enabled_default: true
    }
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
  }
}

