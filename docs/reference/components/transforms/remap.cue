package metadata

components: transforms: "remap": {
	title: "Remap"

	description: """
		Is the recommended transform for parsing, shaping, and transforming data in Vector. It implements the
		[Vector Remap Language](\(urls.vrl_reference)) (VRL), an expression-oriented language designed for processing
		observability data (logs and metrics) in a safe and performant manner.

		Please refer to the [VRL reference](\(urls.vrl_reference)) when writing VRL scripts.
		"""

	classes: {
		commonly_used: true
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		program: {
			runtime: {
				name:    "Vector Remap Language (VRL)"
				url:     urls.vrl_reference
				version: null
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
		warnings: []
		notices: []
	}

	configuration: {
		source: {
			description: """
				The [Vector Remap Language](\(urls.vrl_reference)) (VRL) program to execute for each event.
				"""
			required:    true
			type: string: {
				examples: [
					"""
						. = parse_json(.message)
						.new_field = "new value"
						.status = to_int(.status)
						.duration = parse_duration(.duration, "s")
						.new_name = .old_name
						del(.old_name)
						""",
				]
				syntax: "remap_program"
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
		for k, v in remap.examples if v.raises == _|_ {
			{
				title: v.title
				configuration: source: v.source
				input:  v.input
				output: v.output
			}
		},
	]

	how_it_works: {
		remap_language: {
			title: "Vector Remap Language"
			body:  #"""
				The Vector Remap Language (VRL) is a restrictive, fast, and safe language we
				designed specifically for mapping observability data. It avoids the need to
				chain together many fundamental Vector transforms to accomplish rudimentary
				reshaping of data.

				The intent is to offer the same robustness of full language runtime (ex: Lua)
				without paying the performance or safety penalty.

				Learn more about Vector's Remap Language in the
				[Vector Remap Language reference](\#(urls.vrl_reference)).
				"""#
		}
	}

	telemetry: metrics: {
		processing_errors_total: components.sources.internal_metrics.output.metrics.processing_errors_total
	}
}
