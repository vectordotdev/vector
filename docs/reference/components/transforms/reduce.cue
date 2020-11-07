package metadata

components: transforms: reduce: {
	title: "Reduce"

	classes: {
		commonly_used: false
		development:   "beta"
		egress_method: "stream"
	}

	features: {
		reduce: {}
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
		ends_when: {
			common:      false
			description: "A condition used to distinguish the final event of a transaction. If this condition resolves to true for an event the transaction it belongs to is immediately flushed."
			required:    false
			warnings: []
			type: object: configuration._conditions
		}
		expire_after_ms: {
			common:      false
			description: "A maximum period of time to wait after the last event is received before a combined event should be considered complete."
			required:    false
			warnings: []
			type: uint: {
				default: 30000
				unit:    "milliseconds"
			}
		}
		flush_period_ms: {
			common:      false
			description: "Controls the frequency that Vector checks for (and flushes) expired events."
			required:    false
			warnings: []
			type: uint: {
				default: 1000
				unit:    "milliseconds"
			}
		}
		group_by: {
			common:      true
			description: "An ordered list of fields by which to group events. Each group is combined independently, allowing you to keep independent events separate. When no fields are specified, all events will be combined in a single group. Events missing a specified field will be combined in their own group."
			required:    false
			warnings: []
			type: array: {
				default: []
				items: type: string: examples: ["request_id", "user_id", "transaction_id"]
			}
		}
		merge_strategies: {
			common: false
			description: """
				A map of field names to custom merge strategies. For each
				field specified this strategy will be used for combining
				events rather than the default behavior.

				The default behavior is as follows:

				1. The first value of a string field is kept, subsequent
				   values are discarded.
				2. For timestamp fields the first is kept and a new field
				   `[field-name]_end` is added with the last received
				   timestamp value.
				3. Numeric values are summed.
				"""
			required: false
			warnings: []
			type: object: {
				examples: [
					{
						method:      "discard"
						path:        "discard"
						duration_ms: "sum"
						query:       "array"
					},
				]
				options: {
					"*": {
						description: "The custom merge strategy to use for a field."
						required:    true
						warnings: []
						type: string: {
							enum: {
								array:          "Each value is appended to an array."
								concat:         "Concatenate each string value (delimited with a space)."
								concat_newline: "Concatenate each string value (delimited with a newline)."
								discard:        "Discard all but the first value found."
								sum:            "Sum all numeric values."
								max:            "The maximum of all numeric values."
								min:            "The minimum of all numeric values."
							}
						}
					}
				}
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	examples: [
		{
			title: "Reduce Rails Logs"
			configuration: {}
			input: [
				{log: {timestamp: "2020-10-07T12:33:21.223543Z", message: "Received GET /path", request_id:                     "abcd1234", request_path:    "/path", request_params: {"key":          "val"}}},
				{log: {timestamp: "2020-10-07T12:33:21.832345Z", message: "Executed query in 5.2ms", request_id:                "abcd1234", query:           "SELECT * FROM table", query_duration_ms: 5.2}},
				{log: {timestamp: "2020-10-07T12:33:22.457423Z", message: "Rendered partial _partial.erb in 2.3ms", request_id: "abcd1234", template:        "_partial.erb", render_duration_ms:       2.3}},
				{log: {timestamp: "2020-10-07T12:33:22.543323Z", message: "Executed query in 7.8ms", request_id:                "abcd1234", query:           "SELECT * FROM table", query_duration_ms: 7.8}},
				{log: {timestamp: "2020-10-07T12:33:22.742322Z", message: "Sent 200 in 15.2ms", request_id:                     "abcd1234", response_status: 200, response_duration_ms:                5.2}},
			]
			output: log: {
				timestamp:     "2020-10-07T12:33:21.223543Z"
				timestamp_end: "2020-10-07T12:33:22.742322Z"
				request_id:    "abcd1234"
				request_path:  "/path"
				request_params: {"key": "val"}
				query_duration_ms:    13.0
				render_duration_ms:   2.3
				status:               200
				response_duration_ms: 5.2
			}
		},
	]

	telemetry: metrics: {
		vector_stale_events_flushed_total: {
			description: "The number of stale events that Vector has flushed."
			type:        "counter"
			tags:        _component_tags
		}
	}
}
