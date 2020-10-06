package metadata

components: transforms: regex_parser: {
	title:             "Regex Parser"
	short_description: "Accepts log events and allows you to parse a log field's value with a [Regular Expression][urls.regex]."
	long_description:  "Accepts log events and allows you to parse a log field's value with a [Regular Expression][urls.regex]."

	classes: {
		commonly_used: true
		function:      "parse"
	}

	features: {
	}

	statuses: {
		development: "stable"
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
	}

	configuration: {
		drop_failed: {
			common:      true
			description: "If the event should be dropped if parsing fails."
			required:    false
			warnings: []
			type: bool: default: false
		}
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
				examples: ["message", "parent.child"]
			}
		}
		overwrite_target: {
			common:      false
			description: "If `target_field` is set and the log contains a field of the same name as the target, it will only be overwritten if this is set to `true`."
			required:    false
			warnings: []
			type: bool: default: true
		}
		patterns: {
			description: "The Regular Expressions to apply. Do not include the leading or trailing `/` in any of the expressions."
			required:    true
			warnings: []
			type: "[string]": {
				examples: [["^(?P<timestamp>[\\\\w\\\\-:\\\\+]+) (?P<level>\\\\w+) (?P<message>.*)$"]]
			}
		}
		target_field: {
			common:      false
			description: "If this setting is present, the parsed fields will be inserted into the log as a sub-object with this name. If a field with the same name already exists, the parser will fail and produce an error."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["root_field", "parent.child"]
			}
		}
		types: {
			common:      true
			description: "Key/value pairs representing mapped log field names and types. This is used to coerce log fields into their proper types."
			required:    false
			warnings: []
			type: object: {
				examples: [{"status": "int"}, {"duration": "float"}, {"success": "bool"}, {"timestamp": "timestamp|%F"}, {"timestamp": "timestamp|%a %b %e %T %Y"}, {"parent": {"child": "int"}}]
				options: {}
			}
		}
	}

	examples: log: [
		{
			title: "Syslog 5424"
			configuration: {
				field: "message"
				patterns: [#"^(?P<host>[\w\.]+) - (?P<user>[\w]+) (?P<bytes_in>[\d]+) \[(?P<timestamp>.*)\] "(?P<method>[\w]+) (?P<path>.*)" (?P<status>[\d]+) (?P<bytes_out>[\d]+)$"#]
				types: {
					bytes_in:  "int"
					timestamp: "timestamp|%d/%m/%Y:%H:%M:%S %z"
					status:    "int"
					bytes_out: "int"
				}
			}
			input: {
				"message": #"5.86.210.12 - zieme4647 5667 [19/06/2019:17:20:49 -0400] "GET /embrace/supply-chains/dynamic/vertical" 201 20574"#
			}
			output: {
				bytes_in:  5667
				host:      "5.86.210.12"
				user_id:   "zieme4647"
				timestamp: "2019-06-19T17:20:49-0400"
				method:    "GET"
				path:      "/embrace/supply-chains/dynamic/vertical"
				status:    201
				bytes_out: 20574
			}
		},
	]

	how_it_works: {
		failed_parsing: {
			title: "Failed Parsing"
			body: #"""
				By default, if the input message text does not match any of the configured regular expression patterns, this transform will log an error message but leave the log event unchanged. If you instead wish to have this transform drop the event, set `drop_failed = true`.
				"""#
		}
		regex_debugger: {
			title: "Regex Debugger"
			body: #"""
				If you are having difficulty with your regular expression not matching text, you may try debugging your patterns at [Regex 101][regex_tester]. This site includes a regular expression tester and debugger. The regular expression engine used by Vector is most similar to the "Go" implementation, so make sure that is selected in the "Flavor" menu.
				"""#
		}
		regex_syntax: {
			title: "Regex Syntax"
			body: #"""
				Vector uses the Rust standard regular expression engine for pattern matching. Its syntax shares most of the features of Perl-style regular expressions, with a few exceptions. You can find examples of patterns in the [Rust regex module documentation][rust_regex_syntax].
				"""#
		}
	}
}
