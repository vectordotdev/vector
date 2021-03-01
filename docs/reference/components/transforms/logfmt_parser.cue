package metadata

components: transforms: logfmt_parser: {
	title: "Logfmt Parser"

	description: """
		Parses a log field's value in the [logfmt](\(urls.logfmt)) format.
		"""

	classes: {
		commonly_used: false
		development:   "deprecated"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		parse: {
			format: {
				name:     "Logfmt"
				url:      urls.logfmt
				versions: null
			}
		}
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":      true
			"aarch64-unknown-linux-musl":     true
			"armv7-unknown-linux-gnueabihf":  true
			"armv7-unknown-linux-musleabihf": true
			"x86_64-apple-darwin":            true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: []
		warnings: [
			"""
			\(logfmt_parser._remap_deprecation_notice)

			```vrl
			.message = parse_key_value(.message)
			```
			""",
		]
		notices: []
	}

	configuration: {
		drop_field: {
			common:      true
			description: "If the specified `field` should be dropped (removed) after parsing."
			required:    false
			warnings: []
			type: bool: default: true
		}
		field: {
			common:      true
			description: "The log field to parse."
			required:    false
			warnings: []
			type: string: {
				default: "message"
				examples: ["message", "parent.child", "array[0]"]
				syntax: "literal"
			}
		}
		timezone: configuration._timezone
		types:    configuration._types
	}

	input: {
		logs:    true
		metrics: null
	}

	examples: [
		{
			title: "Heroku Router Log"
			configuration: {
				field:      "message"
				drop_field: true
				types: {
					bytes:  "int"
					status: "int"
				}
			}
			input: log: {
				"message": #"at=info method=GET path=/ host=myapp.herokuapp.com request_id=8601b555-6a83-4c12-8269-97c8e32cdb22 fwd="204.204.204.204" dyno=web.1 connect=1ms service=18ms status=200 bytes=13 tls_version=tls1.1 protocol=http"#
			}
			output: log: {
				"at":          "info"
				"method":      "GET"
				"path":        "/"
				"host":        "myapp.herokuapp.com"
				"request_id":  "8601b555-6a83-4c12-8269-97c8e32cdb22"
				"fwd":         "204.204.204.204"
				"dyno":        "web.1"
				"connect":     "1ms"
				"service":     "18ms"
				"status":      200
				"bytes":       13
				"tls_version": "tls1.1"
				"protocol":    "http"
			}
		},
		{
			title: "Loosely Structured"
			configuration: {
				field:      "message"
				drop_field: false
				types: {
					status: "int"
				}
			}
			input: log: {
				"message": #"info | Sent 200 in 54.2ms duration=54.2ms status=200"#
			}
			output: log: {
				"message":  "info | Sent 200 in 54.2ms duration=54.2ms status=200"
				"duration": "54.2ms"
				"status":   200
			}
		},
	]

	how_it_works: {
		key_value_parsing: {
			title: "Key/Value Parsing"
			body:  """
				This transform can be used for key/value parsing. [Logfmt](\(urls.logfmt)) refers
				to a _loosely_ defined spec that parses a key/value pair delimited by a `=`
				character. This section, and it's keywords, is primarily added to assist users
				in finding this transform for these terms.
				"""
		}

		quoting_values: {
			title: "Quoting Values"
			body: #"""
				Values can be quoted to capture spaces, and quotes can be escaped with `\`.
				For example

				```text
				key1="value with spaces" key2="value with spaces and \""
				```

				Would result in the following `log` event:

				```json title="log event"
				{
				  "key1": "value with spaces",
				  "key2": "value with spaces and \""
				}
				```
				"""#
		}

		format_specification: {
			title: "Format Specification"
			body:  """
				[Logfmt](\(urls.logfmt)) is, unfortunately, a very loosely defined format. There
				is no official specification for the format and Vector makes a best effort to
				parse key/value pairs delimited with a `=`. It works by splitting the `field`'s
				value on non-quoted white-space and then splitting each token by a non-quoted
				`=` character. This makes the parsing process somewhat flexible in that the
				string does not need to be strictly formatted.

				For example, the following log line:

				```js title="log event"
				{
				  "message": "Hello world duration=2s user-agent=\"Firefox/47.3 Mozilla/5.0\""
				}
				```

				Will be successfully parsed into:

				```js title="log event"
				{
				  "message": "Hello world duration=2s user-agent=\"Firefox/47.3 Mozilla/5.0\"",
				  "duration": "2s",
				  "user-agent": "Firefox/47.3 Mozilla/5.0"
				}
				```
				"""
		}
	}

	telemetry: metrics: {
		processing_errors_total: components.sources.internal_metrics.output.metrics.processing_errors_total
	}
}
