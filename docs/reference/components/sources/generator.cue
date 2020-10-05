package metadata

components: sources: generator: {
  title: "#{component.title}"
  short_description: "Ingests data through an internal data generator and outputs log events."
  long_description: "Ingests data through an internal data generator and outputs log events."

  _features: {
    checkpoint: enabled: false
    multiline: enabled: false
    tls: enabled: false
  }

  classes: {
    commonly_used: false
    deployment_roles: ["daemon","service","sidecar"]
    function: "test"
  }

  statuses: {
    delivery: "at_least_once"
    development: "stable"
  }

  support: {

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
    batch_interval: {
      common: false
      description: "The amount of time, in seconds, to pause between each batch of output lines. If not set, there will be no delay."
      required: false
      warnings: []
      type: float: {
        default: null
        examples: [1.0]
      }
    }
    count: {
      common: false
      description: "The number of times to repeat outputting the `lines`."
      required: false
      warnings: []
      type: uint: {
        default: 0
        unit: null
      }
    }
    lines: {
      description: "The list of lines to output."
      required: true
      warnings: []
      type: "[string]": {
        examples: [["Line 1","Line 2"]]
      }
    }
    sequence: {
      common: false
      description: "If `true`, each output line will start with an increasing sequence number."
      required: false
      warnings: []
      type: bool: default: false
    }
  }
}
