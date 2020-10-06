package metadata

components: sources: stdin: {
  title: "#{component.title}"
  short_description: "Ingests data through [standard input (STDIN)][urls.stdin] and outputs log events."
  long_description: "Ingests data through [standard input (STDIN)][urls.stdin] and outputs log events."

  _features: {
    checkpoint: enabled: false
    multiline: enabled: false
    tls: enabled: false
  }

  classes: {
    commonly_used: false
    deployment_roles: ["sidecar"]
    function: "receive"
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
    host_key: {
      common: false
      description: "The key name added to each event representing the current host. This can also be globally set via the [global `host_key` option][docs.reference.global-options#host_key]."
      required: false
      warnings: []
      type: string: default: "host"
    }
    max_length: {
      common: false
      description: "The maximum bytes size of a message before it is discarded."
      required: false
      warnings: []
      type: uint: {
        default: 102400
        unit: "bytes"
      }
    }
  }
}
