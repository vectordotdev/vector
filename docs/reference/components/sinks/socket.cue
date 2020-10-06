package metadata

components: sinks: socket: {
  title: "Socket"
  short_description: "Streams log events to a [socket][urls.socket], such as a [TCP][urls.tcp], [UDP][urls.udp], or [UDS][urls.uds] socket."
  long_description: "Streams log events to a [socket][urls.socket], such as a [TCP][urls.tcp], [UDP][urls.udp], or [UDS][urls.uds] socket."

  classes: {
    commonly_used: true
    function: "transmit"
    service_providers: []
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
      enabled_default: false
    }
  }

  statuses: {
    delivery: "best_effort"
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
    address: {
      description: "The address to connect to. The address _must_ include a port."
      groups: ["tcp","udp"]
      required: true
      warnings: []
      type: string: {
        examples: ["92.12.333.224:5000"]
      }
    }
    mode: {
      description: "The type of socket to use."
      groups: ["tcp","udp","unix"]
      required: true
      warnings: []
      type: string: {
        enum: {
          tcp: "TCP socket"
          udp: "UDP socket"
          unix: "Unix domain socket"
        }
      }
    }
    path: {
      description: "The unix socket path. This should be the absolute path."
      groups: ["unix"]
      required: true
      warnings: []
      type: string: {
        examples: ["/path/to/socket"]
      }
    }
  }
}

