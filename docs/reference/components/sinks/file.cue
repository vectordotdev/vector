package metadata

components: sinks: file: {
  title: "#{component.title}"
  short_description: "Streams log events to a file."
  long_description: "Streams log events to a file."

  _features: {
    batch: enabled: false
    buffer: enabled: false
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
    request: enabled: false
    tls: enabled: false
  }

  classes: {
    commonly_used: false
    function: "transmit"
    service_providers: []
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
    idle_timeout_secs: {
      common: false
      description: "The amount of time a file can be idle  and stay open. After not receiving any events for this timeout, the file will be flushed and closed.\n"
      required: false
      warnings: []
      type: uint: {
        default: "30"
      }
    }
    path: {
      description: "File name to write events to."
      required: true
      warnings: []
      type: string: {
        examples: ["/tmp/vector-%Y-%m-%d.log","/tmp/application-{{ application_id }}-%Y-%m-%d.log"]
      }
    }
  }
}

