package metadata

components: sources: stdin: {
	title: "STDIN"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["sidecar"]
		development:   "stable"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		auto_generated:   true
		acknowledgements: false
		multiline: enabled: false
		codecs: {
			enabled:         true
			default_framing: "`newline_delimited` for codecs other than `native`, which defaults to `length_delimited`"
		}
		receive: {
			from: {
				service: services.stdin
				interface: stdin: {}
			}

			tls: enabled: false
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: base.components.sources.stdin.configuration

	output: logs: line: {
		description: "An individual event from STDIN."
		fields: {
			host:      fields._local_host
			message:   fields._raw_line
			timestamp: fields._current_timestamp
			source_type: {
				description: "The name of the source type."
				required:    true
				type: string: {
					examples: ["stdin"]
				}
			}
		}
	}

	examples: [
		{
			_line: """
				2019-02-13T19:48:34+00:00 [info] Started GET "/" for 127.0.0.1
				"""
			title: "STDIN line"
			configuration: {}
			input: _line
			output: log: {
				timestamp:   _values.current_timestamp
				message:     _line
				host:        _values.local_host
				source_type: "stdin"
			}
		},
	]

	how_it_works: {
		line_delimiters: {
			title: "Line Delimiters"
			body: """
				Each line is read until a new line delimiter, the `0xA` byte, is found.
				"""
		}
	}

	telemetry: metrics: {
		stdin_reads_failed_total: components.sources.internal_metrics.output.metrics.stdin_reads_failed_total
	}
}
