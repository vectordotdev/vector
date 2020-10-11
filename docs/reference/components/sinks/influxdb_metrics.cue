package metadata

components: sinks: influxdb_metrics: {
	title:             "InfluxDB Metrics"
	short_description: "Batches metric events to [InfluxDB][urls.influxdb] using [v1][urls.influxdb_http_api_v1] or [v2][urls.influxdb_http_api_v2] HTTP API."
	long_description:  "[InfluxDB][urls.influxdb] is an open-source time series database developed by InfluxData. It is written in Go and optimized for fast, high-availability storage and retrieval of time series data in fields such as operations monitoring, application metrics, Internet of Things sensor data, and real-time analytics."

	classes: {
		commonly_used: false
		egress_method: "batch"
		function:      "transmit"
		service_providers: ["InfluxData"]
	}

	features: {
		batch: {
			enabled:      true
			common:       false
			max_bytes:    null
			max_events:   20
			timeout_secs: 1
		}
		buffer: enabled:      false
		compression: enabled: false
		encoding: codec: enabled: false
		healthcheck: enabled: true
		request: {
			enabled:                    true
			in_flight_limit:            5
			rate_limit_duration_secs:   1
			rate_limit_num:             5
			retry_initial_backoff_secs: 1
			retry_max_duration_secs:    10
			timeout_secs:               60
		}
		tls: enabled: false
	}

	statuses: {
		delivery:    "at_least_once"
		development: "beta"
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
		warnings: []
		notices: []
	}

	configuration: {
		bucket: {
			description: "The destination bucket for writes into InfluxDB 2."
			groups: ["v2"]
			required: true
			warnings: []
			type: string: {
				examples: ["vector-bucket", "4d2225e4d3d49f75"]
			}
		}
		consistency: {
			common:      true
			description: "Sets the write consistency for the point for InfluxDB 1."
			groups: ["v1"]
			required: false
			warnings: []
			type: string: {
				default: null
				examples: ["any", "one", "quorum", "all"]
			}
		}
		database: {
			description: "Sets the target database for the write into InfluxDB 1."
			groups: ["v1"]
			required: true
			warnings: []
			type: string: {
				examples: ["vector-database", "iot-store"]
			}
		}
		namespace: {
			common:      true
			description: "A prefix that will be added to all metric names."
			groups: ["v1", "v2"]
			required: false
			warnings: []
			type: string: {
				default: null
				examples: ["service"]
			}
		}
		org: {
			description: "Specifies the destination organization for writes into InfluxDB 2."
			groups: ["v2"]
			required: true
			warnings: []
			type: string: {
				examples: ["my-org", "33f2cff0a28e5b63"]
			}
		}
		password: {
			common:      true
			description: "Sets the password for authentication if you’ve enabled authentication for the write into InfluxDB 1."
			groups: ["v1"]
			required: false
			warnings: []
			type: string: {
				default: null
				examples: ["${INFLUXDB_PASSWORD}", "influxdb4ever"]
			}
		}
		retention_policy_name: {
			common:      true
			description: "Sets the target retention policy for the write into InfluxDB 1."
			groups: ["v1"]
			required: false
			warnings: []
			type: string: {
				default: null
				examples: ["autogen", "one_day_only"]
			}
		}
		token: {
			description: "[Authentication token][urls.influxdb_authentication_token] for InfluxDB 2."
			groups: ["v2"]
			required: true
			warnings: []
			type: string: {
				examples: ["${INFLUXDB_TOKEN}", "ef8d5de700e7989468166c40fc8a0ccd"]
			}
		}
		username: {
			common:      true
			description: "Sets the username for authentication if you’ve enabled authentication for the write into InfluxDB 1."
			groups: ["v1"]
			required: false
			warnings: []
			type: string: {
				default: null
				examples: ["todd", "vector-source"]
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
			set:          true
			summary:      true
		}
	}

	examples: metric: [
		{
			_host:  _values.local_host
			_value: 1.5
			title:  "Counter"
			configuration: {}
			input: {
				counter: {
					value: _value
				}
				tags: {
					host: _host
				}
			}
			output: "ns.total,metric_type=counter,host=\(_host) value=\(_value) 1542182950000000011"
		},
		{
			_host: _values.local_host
			title: "Distribution"
			configuration: {}
			input: {
				distribution: {
					values: [1.0, 5.0, 3.0]
					sample_rates: [1.5, 2.0, 3.0]
					statistic: "histogram"
				}
				tags: {
					host: _host
				}
			}
			output: "ns.sparse_stats,metric_type=distribution,host=\(_host) avg=3,count=3,max=5,median=3,min=1,quantile_0.95=4,sum=9 1542182950000000011"
		},
		{
			_host:  _values.local_host
			_value: 1.5
			title:  "Gauge"
			configuration: {}
			input: {
				gauge: {
					value: _value
				}
				tags: {
					host: _host
				}
			}
			output: "ns.meter,metric_type=gauge,host=\(_host) value=\(_value) 1542182950000000011"
		},
		{
			_host: _values.local_host
			title: "Histogram"
			configuration: {}
			input: {
				histogram: {
					buckets: [1.0, 2.1, 3.0]
					counts: [2, 5, 10]
					count: 17
					sum:   46.2
				}
				tags: {
					host: _host
				}
			}
			output: "ns.requests,metric_type=histogram,host=\(_host) bucket_1=2i,bucket_2.1=5i,bucket_3=10i,count=17i,sum=46.2 1542182950000000011"
		},
		{
			_host:  _values.local_host
			_value: 1.5
			title:  "Set"
			configuration: {}
			input: {
				set: {
					values: ["first", "another", "last"]
				}
				tags: {
					host: _host
				}
			}
			output: "ns.users,metric_type=set,host=\(_host) value=3 154218295000000001"
		},
		{
			_host: _values.local_host
			title: "Summary"
			configuration: {}
			input: {
				summary: {
					quantiles: [0.01, 0.5, 0.99]
					values: [1.5, 2.0, 3.0]
					count: 6
					sum:   12.1
				}
				tags: {
					host: _host
				}
			}
			output: "ns.requests,metric_type=summary,host=\(_host) count=6i,quantile_0.01=1.5,quantile_0.5=2,quantile_0.99=3,sum=12.1 1542182950000000011"
		},
	]
}
