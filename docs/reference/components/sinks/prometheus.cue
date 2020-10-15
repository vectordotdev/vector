package metadata

components: sinks: prometheus: {
	title:       "Prometheus"
	description: "[Prometheus][urls.prometheus] is a pull-based monitoring system that scrapes metrics from configured endpoints, stores them efficiently, and supports a powerful query language to compose dynamic information from a variety of otherwise unrelated data points."

	classes: {
		commonly_used: true
		delivery:      "best_effort"
		development:   "beta"
		egress_method: "aggregate"
		service_providers: []
	}

	features: {
		buffer: enabled:      false
		healthcheck: enabled: false
		exposes: {}
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

		requirements: [
			#"""
				[Prometheus][urls.prometheus] version `>= 1.0` is required.
				"""#,
		]
		warnings: [
			#"""
				High cardinality metric names and labels are discouraged by
				Prometheus as they can provide performance and reliability
				problems. You should consider alternative strategies to reduce
				the cardinality. Vector offers a [`tag_cardinality_limit` transform][docs.transforms.tag_cardinality_limit]
				as a way to protect against this.
				"""#,
			#"""
				This component exposes a configured port. You must ensure your
				network allows access to this port.
				"""#,
		]
		notices: []
	}

	configuration: {
		address: {
			description: "The address to expose for scraping."
			required:    true
			warnings: []
			type: string: {
				examples: ["0.0.0.0:9598"]
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
		flush_period_secs: {
			common:      false
			description: "Time interval between [set][docs.data-model.metric#set] values are reset."
			required:    false
			warnings: []
			type: uint: {
				default: 60
				unit:    "seconds"
			}
		}
		namespace: {
			common:      true
			description: "A prefix that will be added to all metric names.\nIt should follow Prometheus [naming conventions][urls.prometheus_metric_naming]."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["service"]
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
		{
			_host:  _values.local_host
			_name:  "logins"
			_value: 1.5
			title:  "Counter"
			configuration: {}
			input: metric: {
				name: _name
				counter: {
					value: _value
				}
				tags: {
					host: _host
				}
			}
			output: #"""
				# HELP \(_name) \(_name)
				# TYPE \(_name) counter
				\(_name) \(_value)
				"""#
		},
		{
			_host:  _values.local_host
			_name:  "memory_rss"
			_value: 1.5
			title:  "Gauge"
			configuration: {}
			input: metric: {
				name: _name
				gauge: {
					value: _value
				}
				tags: {
					host: _host
				}
			}
			output: #"""
				# HELP \(_name) \(_name)
				# TYPE \(_name) gauge
				\(_name) \(_value)
				"""#
		},
		{
			_host: _values.local_host
			_name: "response_time_s"
			title: "Histogram"
			configuration: {}
			input: metric: {
				name: _name
				histogram: {
					buckets: [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
					counts: [0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0]
					count: 2
					sum:   0.789
				}
				tags: {
					host: _host
				}
			}
			output: #"""
				# HELP \(_name) \(_name)
				# TYPE \(_name) histogram
				\(_name)_bucket{le="0.005"} 0
				\(_name)_bucket{le="0.01"} 1
				\(_name)_bucket{le="0.025"} 0
				\(_name)_bucket{le="0.05"} 1
				\(_name)_bucket{le="0.1"} 0
				\(_name)_bucket{le="0.25"} 0
				\(_name)_bucket{le="0.5"} 0
				\(_name)_bucket{le="1.0"} 0
				\(_name)_bucket{le="2.5"} 0
				\(_name)_bucket{le="5.0"} 0
				\(_name)_bucket{le="10.0"} 0
				\(_name)_bucket{le="+Inf"} 0
				\(_name)_sum 0.789
				\(_name)_count 2
				"""#
		},
	]

	how_it_works: {
		histogram_buckets: {
			title: "Histogram Buckets"
			body: #"""
				Choosing the appropriate buckets for Prometheus histograms is a complicated
				point of discussion. The [Histograms and Summaries Prometheus \
				guide][urls.prometheus_histograms_guide] provides a good overview of histograms,
				buckets, summaries, and how you should think about configuring them. The buckets
				you choose should align with your known range and distribution of values as
				well as how you plan to report on them. The aforementioned guide provides
				examples on how you should align them.
				"""#
			sub_sections: [
				{
					title: "Default Buckets"
					body: #"""
						The `buckets` option defines the global default buckets for histograms:

						```toml
						<%= component.options.buckets.default %>
						```

						These defaults are tailored to broadly measure the response time (in seconds)
						of a network service. Most likely, however, you will be required to define
						buckets customized to your use case.

						<Alert type="warning">

						Note: These values are in `<%= component.options.buckets.unit %>`, therefore,
						your metric values should also be in `<%= component.options.buckets.unit %>`.
						If this is not the case you should adjust your metric or buckets to coincide.

						</Alert>
						"""#
				},
			]
		}

		memory_usage: {
			title: "Memory Usage"
			body: #"""
				Like other Prometheus instances, the `<%= component.name %>` sink aggregates
				metrics in memory which keeps the memory footprint to a minimum if Prometheus
				fails to scrape the Vector instance over an extended period of time. The
				downside is that data will be lost if Vector is restarted. This is by design of
				Prometheus' pull model approach, but is worth noting if restart Vector
				frequently.
				"""#
		}
	}
}
