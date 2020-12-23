package metadata

components: transforms: remap: {
	title: "Remap"

	description: """
		Transforms events using the [Vector Remap Language](\(urls.vector_remap_language_reference)),
		a fast, safe, self-documenting data mapping language.
		"""

	classes: {
		commonly_used: true
		development:   "beta"
		egress_method: "stream"
	}

	features: {
		program: {
			runtime: {
				name:    "Vector Remap Language (VRL)"
				url:     urls.vrl
				version: null
			}
		}
	}

	support: {
		targets: {
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
		source: {
			description: "The remap source/instruction set to execute for each event"
			required:    true
			type: string: {
				examples: [
					"""
						. = parse_json(.message)
						.status = to_int(.status)
						.duration = parse_duration(.duration, "s")
						.new_field = .old_field
						del(.old_field)
						""",
				]
			}
		}
	}

	input: {
		logs: true
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
	}

	examples: [
		{
			title: "Adding, renaming, and removing fields"
			configuration: {
				source: #"""
					.new_field = "new value"
					.new_field_name = .old_field_name
					del(.old_name)
					"""#
			}
			input: log: {
				old_field_name: "old value"
			}
			output: log: {
				new_field:      "new value"
				new_field_name: "old value"
			}
		},
		{
			title: "Allowlisting fields"
			configuration: {
				source: """
					only_fields(.field1, .field2)
					"""
			}
			input: log: {
				field1: "value1"
				field2: "value2"
				field3: "value3"
			}
			output: log: {
				field1: "value1"
				field2: "value2"
			}
		},
		{
			title: "Checking for the existence of values"
			configuration: source: """
				.has_name = exists(.name)
				del(.name)
				"""
			input: log: name:      "Vector Vic"
			output: log: has_name: true
		},
		{
			title: "Working with strings"
			configuration: source: """
				.message = strip_whitespace(.message)
				.upper = upcase(.message)
				.lower = downcase(.message)
				.has_hello = contains(.lower, "hello")
				.truncated = truncate(.lower, 5, ellipsis = true)
				.ends_with_booper = ends_with(.lower, "booper")
				del(.message)
				"""
			input: log: message: "  hEllo WoRlD   "
			output: log: {
				upper:            "HELLO WORLD"
				lower:            "hello world"
				has_hello:        true
				truncated:        "hello..."
				ends_with_booper: false
			}
		},
		{
			title: "Working with numbers"
			configuration: {
				source: """
					.rounded_temp = round(.temperature)
					.floor_temp = floor(.temperature)
					.ceil_temp = ceil(.temperature)
					"""
			}
			input: log: temperature: 105.1
			output: log: {
				rounded_temp: 105
				floor_temp:   105
				ceil_temp:    106
			}
		},
		{
			title: "Redacting sensitive information"
			configuration: source: """
				.credit_card = redact(.credit_card, filters = ["pattern"], redactor = "full", patterns = [/[0-9]{16}/])
				"""
			input: log: credit_card:  "1234567812345678"
			output: log: credit_card: "****"
		},
		{
			title: "Stripping ANSI characters"
			configuration: {
				source: """
					.text = strip_ansi_escape_codes(.text)
					"""
			}
			input: log: text:  #"\e[46mfoo\e[0m bar"#
			output: log: text: "foo bar"
		},
		{
			title: "Parsing strings using Grok"
			configuration: source: """
				. = parse_grok(.message, "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}")
				del(.message)
				"""
			input: log: message: "2020-10-02T23:22:12.223222Z info Hello world"
			output: log: {
				level:     "info"
				message:   "Hello world"
				timestamp: "2020-10-02T23:22:12.223222Z"
			}
		},
		{
			title: "Parsing JSON"
			configuration: {
				source: ". = parse_json(.message)"
			}
			input: log: {
				message: #"{"key": "val"}"#
			}
			output: log: {
				key: "val"
			}
		},
		{
			title: "Formatting timestamps"
			configuration: source: """
				.timestamp = to_timestamp(.timestamp)
				.unix_time = format_timestamp(.timestamp, format = "%s")
				.iso_8601_with_tz = format_timestamp(.timestamp, format = "%FT%TZ")
				.iso_8601_no_tz = format_timestamp(.timestamp, format = "%FT%T")
				.year = to_int(format_timestamp(.timestamp, format = "%Y"))

				del(.timestamp)
				"""
			input: log: timestamp: "2020-12-23 01:43:54.951696 UTC"
			output: log: {
				unix_time:        "1608687834"
				iso_8601_with_tz: "2020-12-23T01:43:54Z"
				iso_8601_no_tz:   "2020-12-23T01:43:54"
				year:             2020
			}
		},
		{
			title: "Encoding JSON"
			configuration: {
				source: ".message = encode_json(.)"
			}
			input: log: {
				key: "val"
			}
			output: log: {
				message: #"{"key": "val"}"#
			}
		},
		{
			title: "Working with durations"
			configuration: source: """
				.seconds = parse_duration(.time, "s")
				.minutes = parse_duration(.time, "m")
				.milliseconds = parse_duration(.time, "ms")

				del(.time)
				"""
			input: log: time: "480"
			output: log: {
				seconds:      480
				minutes:      8
				milliseconds: 480000
			}
		},
		{
			title: "Coercing values"
			configuration: {
				source: """
					.bool = to_bool(.bool)
					.float = to_float(.float)
					.int = to_int(.int)
					.timestamp = to_timestamp(.timestamp)
					"""
			}
			input: log: {
				bool:      true
				float:     1.234
				int:       1
				timestamp: "2020-10-01T02:22:11.223212Z"
			}
			output: log: {
				bool:      true
				float:     1.234
				int:       1
				timestamp: "2020-10-01T02:22:11.223212Z"
			}
		},
		{
			title: "Parsing Syslog messages"
			configuration: source: """
				. = parse_syslog(.)
				"""
			input: log: message: "<102>1 2020-12-22T15:22:31.111Z vector-user.biz su 2666 ID389 - Something went wrong"
			output: log: {
				appname:   "su"
				facility:  "ntp"
				hostname:  "vector-user.biz"
				message:   "Something went wrong"
				msgid:     "ID389"
				procid:    2666
				severity:  "info"
				timestamp: "2020-12-22 15:22:31.111 UTC"
			}
		},
		{
			title: "Parsing Syslog severity and level"
			configuration: source: """
				.level = to_syslog_level(.level)
				.severity = to_syslog_severity(.severity)
				"""
			input: log: {
				level:    1
				severity: "error"
			}
			output: log: {
				level:    "alert"
				severity: 3
			}
		},
		{
			title: "Working with URLs"
			configuration: source: ".url = parse_url(.url)"
			input: log: {
				url: "https//vector.dev"
			}
			output: log: url: {
				fragment: null
				host:     "vector.dev"
				password: ""
				path:     "/"
				port:     null
				query: {}
				schema:   "https"
				username: ""
			}
		},
		{
			title: "Working with IP addresses"
			configuration: source: """
				.contains_ipv4_address = ip_cidr_contains(.ipv4_address, "192.168.0.0/16")
				.ipv4_subnet = ip_subnet(.ipv4_address, "255.255.255.0")
				.contains_ipv6_address = ip_cidr_contains(.ipv6_address, "2001:4f8:4:ba::")
				.ipv6_subnet = ip_subnet(.ipv6_address, "/32")
				.ipv4_converted = ipv6_to_ipv4("::ffff:192.168.0.1")
				.ipv6_converted = ip_to_ipv6("localhost")
				"""
			input: log: {
				ipv4_address: "192.168.10.32"
				ipv6_address: "2404:6800:4003:c02::64"
			}
			output: log: {
				contains_ipv4_address: true
				ipv4_subnet:           "192.168.10.0"
				contains_ipv6_address: false
				ipv6_subnet:           "2001:4f8::"
				ipv4_converted:        "192.168.0.1"
				ipv6_converted:        "::ffff:192.169.0.1"
			}
		},
	]

	how_it_works: {
		remap_language: {
			title: "Remap Language"
			body: #"""
				The remap language is a restrictive, fast, and safe language we
				designed specifically for mapping data. It avoids the need to chain
				together many fundamental transforms to accomplish rudimentary
				reshaping of data.

				The intent is to offer the same robustness of full language runtime
				without paying the performance or safety penalty.

				Learn more about Vector's remap syntax in
				[the docs](/docs/reference/remap).
				"""#
		}
	}

	telemetry: metrics: {
		processing_errors_total: components.sources.internal_metrics.output.metrics.processing_errors_total
	}
}
