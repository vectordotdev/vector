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
	decoding: {
		description: """
			Configures how events are decoded from raw bytes. Note some decoders can also determine the event output
			type (log, metric, trace).
			"""
		required: false
		type:     _schemaDefinitions["derived::codecs::decoding::DeserializerConfig::2c0db0ef1f05c78303bd4391"]
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
	framing: {
		description: """
			Framing configuration.

			Framing handles how events are separated when encoded in a raw byte form, where each event is
			a frame that must be prefixed, or delimited, in a way that marks where an event begins and
			ends within the byte stream.
			"""
		required: false
		type:     _schemaDefinitions["derived::codecs::decoding::FramingConfig::2a4f9b813a8f495175f80ccf"]
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
