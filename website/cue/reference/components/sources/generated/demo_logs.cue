package metadata

generated: components: sources: demo_logs: configuration: {
	count: {
		description: """
			The total number of lines to output.

			By default, the source continuously prints logs (infinitely).
			"""
		required: false
		type: uint: default: 9223372036854775807
	}
	format: {
		description: "The format of the randomly generated output."
		required:    true
		type: string: enum: {
			apache_common: """
				Randomly generated logs in [Apache common][apache_common] format.

				[apache_common]: https://httpd.apache.org/docs/current/logs.html#common
				"""
			apache_error: """
				Randomly generated logs in [Apache error][apache_error] format.

				[apache_error]: https://httpd.apache.org/docs/current/logs.html#errorlog
				"""
			bsd_syslog: """
				Randomly generated logs in Syslog format ([RFC 3164][syslog_3164]).

				[syslog_3164]: https://tools.ietf.org/html/rfc3164
				"""
			json: """
				Randomly generated HTTP server logs in [JSON][json] format.

				[json]: https://en.wikipedia.org/wiki/JSON
				"""
			shuffle: "Lines are chosen at random from the list specified using `lines`."
			syslog: """
				Randomly generated logs in Syslog format ([RFC 5424][syslog_5424]).

				[syslog_5424]: https://tools.ietf.org/html/rfc5424
				"""
		}
	}
	interval: {
		description: """
			The amount of time, in seconds, to pause between each batch of output lines.

			The default is one batch per second. To remove the delay and output batches as quickly as possible, set
			`interval` to `0.0`.
			"""
		required: false
		type: float: {
			default: 1.0
			examples: [1.0, 0.1, 0.01]
			unit: "seconds"
		}
	}
	lines: {
		description:   "The list of lines to output."
		relevant_when: "format = \"shuffle\""
		required:      true
		type: array: items: type: string: examples: ["line1", "line2"]
	}
	sequence: {
		description:   "If `true`, each output line starts with an increasing sequence number, beginning with 0."
		relevant_when: "format = \"shuffle\""
		required:      false
		type: bool: default: false
	}
}

generated: components: sources: demo_logs: configuration: decoding: decodingBase & {
	type: object: options: codec: {
		required: false
		type: string: default: "bytes"
	}
}
generated: components: sources: demo_logs: configuration: framing: framingDecoderBase & {
	type: object: options: method: {
		required: false
		type: string: default: "bytes"
	}
}
