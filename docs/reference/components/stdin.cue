package metadata

import (
  "strings"
)

components: sources: stdin: {
  title: "#{component.title}"
  short_description: strings.ToTitle(classes.function) + " log [standard input (STDIN)][urls.stdin]"
  description: null

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
    development: "prod-ready"
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
        type: string: {
          default: "host"
        }
    }
    max_length: {
      common: false
      description: "The maximum bytes size of a message before it is discarded."
      required: false
        type: uint: {
          default: 102400
          unit: "bytes"
        }
    }
  }
}
