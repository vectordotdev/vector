package metadata

components: transforms: reduce: {
	title: "Reduce"

	description: """
		Reduces multiple log events into a single log event based on a set of
		conditions and merge strategies.
		"""

	classes: {
		commonly_used: false
		development:   "beta"
		egress_method: "stream"
		stateful:      true
	}

	features: {
		reduce: {}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		ends_when: {
			common: false
			description: """
				A condition used to distinguish the final event of a transaction. If this condition resolves to `true`
				for an event, the current transaction is immediately flushed with this event.
				"""
			required: false
			type: string: {
				default: null
				examples: [
					#".status_code != 200 && !includes(["info", "debug"], .severity)"#,
				]
			}
		}
		expire_after_ms: {
			common:      false
			description: "A maximum period of time to wait after the last event is received before a combined event should be considered complete."
			required:    false
			type: uint: {
				default: 30000
				unit:    "milliseconds"
			}
		}
		flush_period_ms: {
			common:      false
			description: "Controls the frequency that Vector checks for (and flushes) expired events."
			required:    false
			type: uint: {
				default: 1000
				unit:    "milliseconds"
			}
		}
		group_by: {
			common:      true
			description: "An ordered list of fields by which to group events. Each group is combined independently, allowing you to keep independent events separate. When no fields are specified, all events will be combined in a single group. Events missing a specified field will be combined in their own group."
			required:    false
			type: array: {
				default: []
				items: type: string: {
					examples: ["request_id", "user_id", "transaction_id"]
				}
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
						type: string: {
							enum: {
								array:          "Each value is appended to an array."
								longest_array:  "Retains the longest array seen"
								shortest_array: "Retains the shortest array seen"
								concat:         "Concatenate each string value (delimited with a space)."
								concat_newline: "Concatenate each string value (delimited with a newline)."
								discard:        "Discard all but the first value found."
								retain:         "Discard all but the last value found. Works as a coalesce by not retaining null."
								sum:            "Sum all numeric values."
								max:            "The maximum of all numeric values."
								min:            "The minimum of all numeric values."
								flat_unique:    "Create a flattened array of all the unique values."
							}
						}
					}
				}
			}
		}
		starts_when: {
			common: false
			description: """
				A condition used to distinguish the first event of a transaction. If this condition resolves to `true`
				for an event, the previous transaction is flushed (without this event) and a new transaction is started.
				"""
			required: false
			type: condition: {}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	examples: [
		{
			title: "Merge Ruby exceptions"
			input: [
				{
					log: {
						timestamp: "2020-10-07T12:33:21.223543Z"
						message:   "foobar.rb:6:in `/': divided by 0 (ZeroDivisionError)"
						host:      "host-1.hostname.com"
						pid:       1234
						tid:       5678
					}
				},
				{
					log: {
						timestamp: "2020-10-07T12:33:21.223543Z"
						message:   "    from foobar.rb:6:in `bar'"
						host:      "host-1.hostname.com"
						pid:       1234
						tid:       5678
					}
				},
				{
					log: {
						timestamp: "2020-10-07T12:33:21.223543Z"
						message:   "    from foobar.rb:2:in `foo'"
						host:      "host-1.hostname.com"
						pid:       1234
						tid:       5678
					}
				},
				{
					log: {
						timestamp: "2020-10-07T12:33:21.223543Z"
						message:   "    from foobar.rb:9:in `<main>'"
						host:      "host-1.hostname.com"
						pid:       1234
						tid:       5678
					}
				},
				{
					log: {
						timestamp: "2020-10-07T12:33:22.123528Z"
						message:   "Hello world, I am a new log"
						host:      "host-1.hostname.com"
						pid:       1234
						tid:       5678
					}
				},
			]

			configuration: {
				group_by: ["host", "pid", "tid"]
				merge_strategies: {
					message: "concat_newline"
				}
				starts_when: #"match(.message, /^[^\s]/)"#
			}

			output: [
				{
					log: {
						timestamp: "2020-10-07T12:33:21.223543Z"
						message: """
							foobar.rb:6:in `/': divided by 0 (ZeroDivisionError)
							    from foobar.rb:6:in `bar'
							    from foobar.rb:2:in `foo'
							    from foobar.rb:9:in `<main>'
							"""
						host: "host-1.hostname.com"
						pid:  1234
						tid:  5678
					}
				},
				{
					log: {
						timestamp: "2020-10-07T12:33:22.123528Z"
						message:   "Hello world, I am a new log"
						host:      "host-1.hostname.com"
						pid:       1234
						tid:       5678
					}
				},
			]
		},
		{
			title: "Reduce Rails logs into a single transaction"

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
		stale_events_flushed_total: components.sources.internal_metrics.output.metrics.stale_events_flushed_total
	}
}
