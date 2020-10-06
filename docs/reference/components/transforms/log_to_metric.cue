package metadata

components: transforms: log_to_metric: {
  title: "Log to Metric"
  short_description: "Accepts log events and allows you to convert logs into one or more metrics."
  long_description: "Accepts log events and allows you to convert logs into one or more metrics."

  classes: {
    commonly_used: true
    function: "convert"
  }

  features: {
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
