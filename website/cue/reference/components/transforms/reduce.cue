package metadata

components: transforms: reduce: {
	title: "Reduce"

	description: """
		Reduces multiple log events into a single log event based on a set of
		conditions and merge strategies.
		"""

	classes: {
		commonly_used: false
		development:   "stable"
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

	configuration: base.components.transforms.reduce.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
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
				starts_when: #"match(string!(.message), r'^[^\s]')"#
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
