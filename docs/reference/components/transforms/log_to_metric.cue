package metadata

components: transforms: log_to_metric: {
	title:             "Log to Metric"
	short_description: "Accepts log events and allows you to convert logs into one or more metrics."

	classes: {
		commonly_used: false
		development:   "stable"
		egress_method: "batch"
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
		metrics: {
			description: "A table of key/value pairs representing the keys to be added to the event."
			required:    true
			warnings: []
			type: object: {
				examples: []
				options: {
					field: {
						description: "The log field to use as the metric."
						required:    true
						warnings: []
						type: string: {
							examples: ["duration", "parent.child"]
						}
					}
					increment_by_value: {
						description: """
                If `true` the metric will be incremented by the `field` value.
                If `false` the metric will be incremented by 1 regardless of the `field` value.
                """
						required: false
						common:   false
						warnings: []
						relevant_when: #"`type` = `"counter"`"#
						type: bool: {
							default: false
						}
					}
					name: {
						description: "The name of the metric. Defaults to `<field>_total` for `counter` and `<field>` for `gauge`."
						required:    true
						warnings: []
						type: string: {
							examples: ["duration_total"]
						}
					}
					tags: {
						description: "Key/value pairs representing [metric tags][docs.data-model.metric#tags]."
						required:    false
						common:      true
						warnings: []
						type: object: {
							examples: [
								{
									host:   "${HOSTNAME}"
									region: "us-east-1"
									status: "{{status}}"
								},
							]
							options: {
								"*": {
									description: """
                      Key/value pairs representing [metric tags][docs.data-model.metric#tags].
                      Environment variables and field interpolation is allowed.
                      """
									required: true
									warnings: []
									type: "*": {}
								}
							}
						}
					}
					type: {
						description: "The metric type."
						required:    true
						warnings: []
						type: string: {
							enum: {
								counter:   "A [counter metric type][docs.data-model.metric#counter]."
								gauge:     "A [gauge metric type][docs.data-model.metric#gauge]."
								histogram: "A [distribution metric type with histogram statistic][docs.data-model.metric#distribution]."
								set:       "A [set metric type][docs.data-model.metric#set]."
								summary:   "A [distribution metric type with summary statistic][docs.data-model.metric#distribution]."
							}
						}
					}
				}
			}
		}
	}

	input: {
		logs:    true
		metrics: false
	}

	output: metrics: {
		counter:   output._passthrough_counter
		gauge:     output._passthrough_gauge
		histogram: output._passthrough_histogram
		set:       output._passthrough_set
		summary:   output._passthrough_summary
	}

	examples: [
		{
			title: "Counters"
			configuration: {
				metrics: [
					{
						type:  "counter"
						field: "status"
						name:  "response_total"
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
			}
			output: [{metric: {
				name: "time_ms"
				tags: {
					status: "200"
					host:   "10.22.11.222"
				}
				counter: {
					value: 1.0
				}
			}}]
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

	how_it_works: {
		multiple_metrics: {
			title: "Multiple Metrics"
			body: """
				For clarification, when you convert a single `log` event into multiple `metric`
				events, the `metric` events are not emitted as a single array. They are emitted
				individually, and the downstream components treat them as individual events.
				Downstream components are not aware they were derived from a single log event.
				"""
		}
		reducing: {
			title: "Reducing"
			body: """
				It's important to understand that this transform does not reduce multiple logs
				to a single metric. Instead, this transform converts logs into granular
				individual metrics that can then be reduced at the edge. Where the reduction
				happens depends on your metrics storage. For example, the
				[`prometheus` sink][docs.sinks.prometheus] will reduce logs in the sink itself
				for the next scrape, while other metrics sinks will proceed to forward the
				individual metrics for reduction in the metrics storage itself.
				"""
		}
		null_fields: {
			title: "Null Fields"
			body: """
				If the target log `field` contains a `null` value it will ignored, and a metric
				will not be emitted.
				"""
		}
	}
}
