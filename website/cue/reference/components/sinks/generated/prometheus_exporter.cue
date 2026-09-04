package metadata

generated: components: sinks: prometheus_exporter: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	address: {
		description: """
			The address to expose for scraping.

			The metrics are exposed at the typical Prometheus exporter path, `/metrics`.
			"""
		required: false
		type: string: {
			default: "0.0.0.0:9598"
			examples: ["192.160.0.10:9598"]
		}
	}
	auth: {
		description: """
			Configuration of the authentication strategy for HTTP requests.

			HTTP authentication should be used with HTTPS only, as the authentication credentials are passed as an
			HTTP header without any additional encryption beyond what is provided by the transport itself.
			"""
		required: false
		type:     _schemaDefinitions["core::option::Option<vector::http::Auth>"]
	}
	buckets: {
		description: """
			Default buckets to use for aggregating [distribution][dist_metric_docs] metrics into histograms.

			[dist_metric_docs]: https://vector.dev/docs/architecture/data-model/metric/#distribution
			"""
		required: false
		type: array: {
			default: [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
			items: type: float: {}
		}
	}
	default_namespace: {
		description: """
			The default namespace for any metrics sent.

			This namespace is only used if a metric has no existing namespace. When a namespace is
			present, it is used as a prefix to the metric name, and separated with an underscore (`_`).

			It should follow the Prometheus [naming conventions][prom_naming_docs].

			[prom_naming_docs]: https://prometheus.io/docs/practices/naming/#metric-names
			"""
		required: false
		type: string: {}
	}
	distributions_as_summaries: {
		description: """
			Whether or not to render [distributions][dist_metric_docs] as an [aggregated histogram][prom_agg_hist_docs] or  [aggregated summary][prom_agg_summ_docs].

			While distributions as a lossless way to represent a set of samples for a
			metric is supported, Prometheus clients (the application being scraped, which is this sink) must
			aggregate locally into either an aggregated histogram or aggregated summary.

			[dist_metric_docs]: https://vector.dev/docs/architecture/data-model/metric/#distribution
			[prom_agg_hist_docs]: https://prometheus.io/docs/concepts/metric_types/#histogram
			[prom_agg_summ_docs]: https://prometheus.io/docs/concepts/metric_types/#summary
			"""
		required: false
		type: bool: default: false
	}
	flush_period_secs: {
		description: """
			The interval, in seconds, on which metrics are flushed.

			On the flush interval, if a metric has not been seen since the last flush interval, it is
			considered expired and is removed.

			Be sure to configure this value higher than your client’s scrape interval.

			Set to `0` to disable expiration entirely. Metrics will then accumulate for as long as the
			sink runs, which can result in unbounded memory growth if metric series cardinality is
			unbounded.
			"""
		required: false
		type: uint: {
			default: 60
			unit:    "seconds"
		}
	}
	quantiles: {
		description: """
			Quantiles to use for aggregating [distribution][dist_metric_docs] metrics into a summary.

			[dist_metric_docs]: https://vector.dev/docs/architecture/data-model/metric/#distribution
			"""
		required: false
		type: array: {
			default: [0.5, 0.75, 0.9, 0.95, 0.99]
			items: type: float: {}
		}
	}
	suppress_timestamp: {
		description: """
			Suppresses timestamps on the Prometheus output.

			This can sometimes be useful when the source of metrics leads to their timestamps being too
			far in the past for Prometheus to allow them, such as when aggregating metrics over long
			time periods, or when replaying old metrics from a disk buffer.
			"""
		required: false
		type: bool: default: false
	}
	tls: {
		description: "Configures the TLS options for incoming/outgoing connections."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsEnableableConfig>"]
	}
}
