package metadata

components: transforms: coercer: {
	title: "Coercer"

	description: """
		Coerces log fields into typed values.
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

	input: {
		logs:    true
		metrics: null
	}

	configuration: {
		drop_unspecified: {
			common:      false
			description: "Set to `true` to drop all fields that are not specified in the `types` table. Make sure both `message` and `timestamp` are specified in the `types` table as their absense will cause the original message data to be dropped along with other extraneous fields."
			required:    false
			warnings: []
			type: bool: default: false
		}
		types: configuration._types
	}

	examples: [
		{
			title: "Date"
			configuration: {
				types: {
					bytes_in:  "int"
					bytes_out: "int"
					status:    "int"
					timestamp: "timestamp|%d/%m/%Y:%H:%M:%S %z"
				}
			}
			input: log: {
				bytes_in:  "5667"
				bytes_out: "20574"
				host:      "5.86.210.12"
				message:   "GET /embrace/supply-chains/dynamic/vertical"
				status:    "201"
				timestamp: "19/06/2019:17:20:49 -0400"
				user_id:   "zieme4647"
			}
			output: log: {
				bytes_in:  5667
				bytes_out: 20574
				host:      "5.86.210.12"
				message:   "GET /embrace/supply-chains/dynamic/vertical"
				status:    201
				timestamp: "19/06/2019:17:20:49 -0400"
				user_id:   "zieme4647"
			}
		},
	]

	telemetry: metrics: {
		processing_errors_total: components.sources.internal_metrics.output.metrics.processing_errors_total
	}
}
