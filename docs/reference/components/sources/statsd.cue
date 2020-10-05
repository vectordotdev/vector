package metadata

components: sources: statsd: {
  title: "#{component.title}"
  short_description: "Ingests data through the [StatsD UDP protocol][urls.statsd_udp_protocol] and outputs metric events."
  long_description: "[StatsD][urls.statsd] is a standard and, by extension, a set of tools that can be used to send, collect, and aggregate custom metrics from any application. Originally, StatsD referred to a daemon written by [Etsy][urls.etsy] in Node."

  _features: {
    checkpoint: enabled: false
    multiline: enabled: false
    tls: enabled: false
  }

  classes: {
    commonly_used: false
    deployment_roles: ["service"]
    function: "receive"
  }

  statuses: {
    delivery: "best_effort"
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

    requirements: [
      """
      This component exposes a configured port. You must ensure your network allows access to this port.
      """,
    ]
    warnings: []
  }

  configuration: {
    address: {
      description: "UDP socket address to bind to."
      required: true
      warnings: []
      type: string: {
        examples: ["127.0.0.1:8126"]
      }
    }
  }
}
