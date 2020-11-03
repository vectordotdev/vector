package metadata

components: sinks: prometheus_remote_write: {
	title:       "Prometheus Remote Write"
	description: "[Prometheus](\(urls.prometheus)) is a monitoring system that scrapes metrics from configured endpoints, stores them efficiently, and supports a powerful query language to compose dynamic information from a variety of otherwise unrelated data points."

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: []
	}

	features: {
		buffer: enabled:      false
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    null
				max_events:   1000
				timeout_secs: 1
			}
			// TODO Snappy is always enabled
			compression: enabled: false
			encoding: enabled:    false
			request: {
				enabled:                    true
				in_flight_limit:            5
				rate_limit_duration_secs:   1
				rate_limit_num:             5
				retry_initial_backoff_secs: 1
				retry_max_duration_secs:    10
				timeout_secs:               60
			}
			tls: {
				enabled:                true
				can_enable:             false
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
			}
			to: {
				name:     "Prometheus"
				thing:    "a \(name) or compatible server"
				url:      urls.prometheus
				versions: null

				interface: {
					socket: {
						api: {
							title: "Prometheus remote_write protocol"
							url:   urls.prometheus_remote_write
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "optional"
					}
				}
			}
		}
	}

	support: {
		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: []
		warnings: [
			"""
				High cardinality metric names and labels are discouraged by
				Prometheus as they can provide performance and reliability
				problems. You should consider alternative strategies to reduce
				the cardinality. Vector offers a [`tag_cardinality_limit` transform][docs.transforms.tag_cardinality_limit]
				as a way to protect against this.
				""",
		]
		notices: []
	}

	configuration: {
		endpoint: {
			description: "The endpoint URL to send data to."
			required:    true
			warnings: []
			type: string: {
				examples: ["https://localhost:8087/"]
			}
		}
		buckets: {
			common:      false
			description: "Default buckets to use for aggregating [distribution][docs.data-model.metric#distribution] metrics into histograms."
			required:    false
			warnings: []
			type: array: {
				default: [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
				items: type: float: examples: [0.005, 0.01]
			}
		}
		quantiles: {
			common:      false
			description: "Quantiles to use for aggregating [distribution][docs.data-model.metric#distribution] metrics into a summary."
			required:    false
			warnings: []
			type: array: {
				default: [0.5, 0.75, 0.9, 0.95, 0.99]
				items: type: float: examples: [0.5, 0.75, 0.9, 0.95, 0.99]
			}
		}
	}

	input: {
		logs: false
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          false
			summary:      true
		}
	}

	examples: [
	]

	how_it_works: {
		histogram_buckets: {
			title: "Histogram Buckets"
			body: #"""
				Choosing the appropriate buckets for Prometheus histograms is a complicated
				point of discussion. The [Histograms and Summaries Prometheus guide](\(urls.prometheus_histograms_guide)) provides a good overview of histograms,
				buckets, summaries, and how you should think about configuring them. The buckets
				you choose should align with your known range and distribution of values as
				well as how you plan to report on them. The aforementioned guide provides
				examples on how you should align them.
				"""#
			sub_sections: [
				{
					title: "Default Buckets"
					body: """
						The `buckets` option defines the global default buckets for histograms.
						These defaults are tailored to broadly measure the response time (in seconds)
						of a network service. Most likely, however, you will be required to define
						buckets customized to your use case.
						"""
				},
			]
		}

		memory_usage: {
			title: "Memory Usage"
			body: """
				Like other Prometheus instances, the `prometheus` sink aggregates
				metrics in memory which keeps the memory footprint to a minimum if Prometheus
				fails to scrape the Vector instance over an extended period of time. The
				downside is that data will be lost if Vector is restarted. This is by design of
				Prometheus' pull model approach, but is worth noting if restart Vector
				frequently.
				"""
		}
	}
}
