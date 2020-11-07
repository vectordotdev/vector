package metadata

components: transforms: remap: {
	title: "Remap"

	classes: {
		commonly_used: true
		development:   "beta"
		egress_method: "stream"
	}

	features: {
		program: {
			runtime: {
				name:    "Timber Remap Language (TRL)"
				url:     urls.trl
				version: null
			}
		}
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
		source: {
			description: "The remap source/instruction set to execute for each event"
			required:    true
			type: string: {
				examples: [
					"""
						.type = "foo",
						.new_field = .old_field * 2
						del(.old_field)
						""",
				]
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	examples: [
		{
			title: "Add, Rename, & Remove Fields"
			configuration: {
				source: #"""
					.new_field = "new value"
					.new_name = .old_name
					del(.old_name)
					"""#
			}
			input: log: {
				old_name: "old value"
			}
			output: log: {
				new_field: "new value"
				new_name:  "old value"
			}
		},
		{
			title: "Parse JSON"
			configuration: {
				source: #"""
					message = del(.message)
					. = parse_json(message)
					"""#
			}
			input: log: {
				message: #"{"key": "val"}"#
			}
			output: log: {
				key: "val"
			}
		},
		{
			title: "Coerce Values"
			configuration: {
				source: #"""
					.bool = to_bool(.bool)
					.float = to_float(.float)
					.int = to_int(.int)
					.timestamp = to_timestamp(.timestamp)
					"""#
			}
			input: log: {
				bool:      "true"
				float:     "1.234"
				int:       "1"
				timestamp: "2020-10-01T02:22:11.223212Z"
			}
			output: log: {
				bool:      true
				float:     1.234
				int:       1
				timestamp: "2020-10-01T02:22:11.223212Z"
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
		vector_processing_errors_total: _vector_processing_errors_total
	}
}
