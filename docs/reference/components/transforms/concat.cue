package metadata

components: transforms: concat: {
	title: "Concat"

	description: """
		Slices log string fields and joins them into a single field.
		"""

	classes: {
		commonly_used: false
		development:   "deprecated"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		shape: {}
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
		warnings: [transforms.add_fields.support.warnings[0]]
		notices: []
	}

	configuration: {
		items: {
			description: "A list of substring definitons in the format of source_field[start..end]. For both start and end negative values are counted from the end of the string."
			required:    true
			warnings: []
			type: array: items: type: string: {
				examples: ["first[..3]", "second[-5..]", "third[3..6]"]
				syntax: "literal"
			}
		}
		joiner: {
			common:      false
			description: "The string that is used to join all items."
			required:    false
			warnings: []
			type: string: {
				default: " "
				examples: [" ", ",", "_", "+"]
				syntax: "literal"
			}
		}
		target: {
			description: "The name for the new label."
			required:    true
			warnings: []
			type: string: {
				examples: ["root_field_name", "parent.child", "array[0]"]
				syntax: "literal"
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	examples: [
		{
			title: "Date"
			configuration: {
				items: ["month", "day", "year"]
				target: "date"
				joiner: "/"
			}
			input: log: {
				message: "Hello world"
				month:   "12"
				day:     "25"
				year:    "2020"
			}
			output: log: {
				message: "Hello world"
				date:    "12/25/2020"
				month:   "12"
				day:     "25"
				year:    "2020"
			}
		},
	]

	telemetry: metrics: {
		processing_errors_total: components.sources.internal_metrics.output.metrics.processing_errors_total
	}
}
