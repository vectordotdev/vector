package metadata

components: sinks: datadog_logs: {
  title: "#{component.title}"
  short_description: "Streams log events to [Datadog's][urls.datadog] logs via the [TCP endpoint][urls.datadog_logs_endpoints]."
  long_description: "[Datadog][urls.datadog] is a monitoring service for cloud-scale applications, providing monitoring of servers, databases, tools, and services, through a SaaS-based data analytics platform."

  _features: {
    batch: enabled: false
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
    request: enabled: false
    tls: {
      enabled: true
      can_enable: true
      can_verify_certificate: true
      can_verify_hostname: true
      enabled_default: true
    }
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
      common: true
      description: "Datadog [API key](https://docs.datadoghq.com/api/?lang=bash#authentication)"
      groups: []
      required: true
      warnings: []
        type: string: {
          examples: ["${DATADOG_API_KEY_ENV_VAR}","ef8d5de700e7989468166c40fc8a0ccd"]
        }
    }
  }
}
