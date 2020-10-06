package metadata

components: sinks: prometheus: {
  title: "Prometheus"
  short_description: "Exposes metric events to [Prometheus][urls.prometheus] metrics service."
  long_description: "[Prometheus][urls.prometheus] is a pull-based monitoring system that scrapes metrics from configured endpoints, stores them efficiently, and supports a powerful query language to compose dynamic information from a variety of otherwise unrelated data points."

  classes: {
    commonly_used: true
    function: "transmit"
    service_providers: []
  }

  features: {
    batch: enabled: false
    buffer: enabled: false
    compression: enabled: false
    encoding: enabled: false
    request: enabled: false
    tls: enabled: false
  }

  statuses: {
    delivery: "best_effort"
    development: "beta"
  }

  support: {
    input_types: ["metric"]

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
      [Prometheus][urls.prometheus] version `>= 1.0` is required.
      """,
    ]
    warnings: []
  }

  configuration: {
    address: {
      description: "The address to expose for scraping."
      required: true
      warnings: []
      type: string: {
        examples: ["0.0.0.0:9598"]
      }
    }
    buckets: {
      common: false
      description: "Default buckets to use for aggregating [distribution][docs.data-model.metric#distribution] metrics into histograms."
      required: false
      warnings: []
    }
    flush_period_secs: {
      common: false
      description: "Time interval between [set][docs.data-model.metric#set] values are reset."
      required: false
      warnings: []
      type: uint: {
        default: 60
        unit: "seconds"
      }
    }
    namespace: {
      common: true
      description: "A prefix that will be added to all metric names.\nIt should follow Prometheus [naming conventions][urls.prometheus_metric_naming]."
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["service"]
      }
    }
  }
}

