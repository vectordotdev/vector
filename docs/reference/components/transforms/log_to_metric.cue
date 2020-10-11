package metadata

components: transforms: log_to_metric: {
	title:             "Log to Metric"
	short_description: "Accepts log events and allows you to convert logs into one or more metrics."
	long_description:  "Accepts log events and allows you to convert logs into one or more metrics."

	classes: {
		commonly_used: true
		function:      "convert"
	}

	features: {
	}

	statuses: {
		development: "stable"
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
						description: "If `true` the metric will be incremented by the `field` value. If `false` the metric will be incremented by 1 regardless of the `field` value."
						required:    false
						common:      false
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
									description: "Key/value pairs representing [metric tags][docs.data-model.metric#tags]. Environment variables and field interpolation is allowed."
									required:    true
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
}
