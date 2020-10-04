package metadata

components: sinks: blackhole: {
  title: "#{component.title}"
  short_description: "Streams log and metric events to a blackhole that simply discards data, designed for testing and benchmarking purposes."
  long_description: "Streams log and metric events to a blackhole that simply discards data, designed for testing and benchmarking purposes."

  _features: {
    batch: enabled: false
    buffer: enabled: false
    checkpoint: enabled: false
    compression: enabled: false
    encoding: enabled: false
    multiline: enabled: false
    request: enabled: false
    tls: enabled: false
  }

  classes: {
    commonly_used: false
    function: "test"
    service_providers: []
  }

  statuses: {
    delivery: "at_least_once"
    development: "stable"
  }

  support: {
      input_types: ["log","metric"]

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
    print_amount: {
      common: true
      description: "The number of events that must be received in order to print a summary of activity."
      required: true
      warnings: []
      type: uint: {
        examples: [1000]
      }
    }
  }
}
