package metadata

components: transforms: logfmt_parser: {
	title:             "Logfmt Parser"
	short_description: "Accepts log events and allows you to parse a log field's value in the [logfmt][urls.logfmt] format."
	long_description:  "Accepts log events and allows you to parse a log field's value in the [logfmt][urls.logfmt] format."

	classes: {
		commonly_used: true
		egress_method: "stream"
		function:      "parse"
	}

	features: {}

	statuses: {
		development: "beta"
	}

	support: {
		input_types: ["log"]

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
			}
		}
		types: components._types
	}

	how_it_works: {
		key_value_parsing: {
			title: "Key/Value Parsing"
			body: #"""
				This transform can be used for key/value parsing. [Logfmt][urls.logfmt] refers
				to a _loosely_ defined spec that parses a key/value pair delimited by a `=`
				character. This section, and it's keywords, is primarily added to assist users
				in finding this transform for these terms.
				"""#
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
			body: #"""
				[Logfmt][urls.logfmt] is, unfortunately, a very loosely defined format. There
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
				"""#
		}
	}
}
