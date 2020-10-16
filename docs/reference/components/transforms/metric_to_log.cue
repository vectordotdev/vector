package metadata

components: transforms: metric_to_log: {
	title: "Metric to Log"

	classes: {
		commonly_used: false
		development:   "stable"
		egress_method: "stream"
	}

	features: {
		convert: {}
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
		host_tag: {
			common:      true
			description: "Tag key that identifies the source host."
			required:    false
			warnings: []
			type: string: {
				default: "hostname"
				examples: ["hostname", "host"]
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

	examples: [
		{
			title: "Metric To Log"
			configuration: {
				host_tag: "host"
			}
			input: metric: {
				name:      "histogram"
				timestamp: "2020-08-01T21:15:47+00:00"
				tags: {
					host: "my.host.com"
					code: "200"
				}
				histogram: {
					buckets: [1.0, 2.0]
					counts: [10, 20]
					count: 30
					sum:   50.0
				}
			}
			output: log: {
				name:      "histogram"
				timestamp: "2020-08-01T21:15:47+00:00"
				host:      "my.host.com"
				tags: {
					"code": "200"
				}
				kind: "absolute"
				aggregated_histogram: {
					buckets: [1.0, 2.0]
					counts: [10, 20]
					count: 30
					sum:   50.0
				}
			}
		},
		{
			title: "Gauge"
			configuration: {
				metrics: [
					{
						type:  "gauge"
						field: "1m_load_avg"
						name:  "1m_load_avg"
						tags: host: "{{host}}"
					},
					{
						type:  "gauge"
						field: "5m_load_avg"
						name:  "5m_load_avg"
						tags: host: "{{host}}"
					},
					{
						type:  "gauge"
						field: "15m_load_avg"
						name:  "15m_load_avg"
						tags: host: "{{host}}"
					},
				]
			}
			input: log: {
				host:           "10.22.11.222"
				message:        "CPU activity sample"
				"1m_load_avg":  78.2
				"5m_load_avg":  56.2
				"15m_load_avg": 48.7
			}
			output: [
				{metric: {
					name: "1m_load_avg"
					tags: {
						host: "10.22.11.222"
					}
					gauge: {
						value: 78.2
					}
				}},
				{metric: {
					name: "5m_load_avg"
					tags: {
						host: "10.22.11.222"
					}
					gauge: {
						value: 56.2
					}
				}},
				{metric: {
					name: "15m_load_avg"
					tags: {
						host: "10.22.11.222"
					}
					gauge: {
						value: 48.7
					}
				}},
			]
		},
		{
			title: "Histograms"
			configuration: {
				metrics: [
					{
						type:  "histogram"
						field: "time"
						name:  "time_ms"
						tags: {
							status: "{{status}}"
							host:   "{{host}}"
						}
					},
				]
			}
			input: log: {
				host:    "10.22.11.222"
				message: "Sent 200 in 54.2ms"
				status:  200
				time:    54.2
			}
			output: [{metric: {
				name: "time_ms"
				tags: {
					status: "200"
					host:   "10.22.11.222"
				}
				distribution: {
					values: [54.2]
					sample_rates: [1.0]
					statistic: "histogram"
				}
			}}]
		},
		{
			title: "Sets"
			configuration: {
				metrics: [
					{
						type:  "histogram"
						field: "time"
						name:  "time_ms"
						tags: {
							status: "{{status}}"
							host:   "{{host}}"
						}
					},
				]
			}
			input: log: {
				host:        "10.22.11.222"
				message:     "Sent 200 in 54.2ms"
				remote_addr: "233.221.232.22"
			}
			output: [{metric: {
				name: "time_ms"
				tags: {
					status: "200"
					host:   "10.22.11.222"
				}
				set: {
					values: ["233.221.232.22"]
				}
			}}]
		},
	]

	how_it_works: {}
}
