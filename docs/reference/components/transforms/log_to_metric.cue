package metadata

components: transforms: log_to_metric: {
  title: "#{component.title}"
  short_description: "Accepts log events and allows you to convert logs into one or more metrics."
  long_description: "Accepts log events and allows you to convert logs into one or more metrics."

  _features: {
    checkpoint: enabled: false
    multiline: enabled: false
    tls: enabled: false
  }

  classes: {
    commonly_used: true
    function: "convert"
  }

  statuses: {
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
    metrics: {
      description: "A table of key/value pairs representing the keys to be added to the event."
      required: true
      warnings: []
    }
  }
}
