package metadata

components: transforms: "remap": {
	title: "Remap"

	description: """
		Is the recommended transform for parsing, shaping, and transforming data in Vector. It implements the
		[Vector Remap Language](\(urls.vrl_reference)) (VRL), an expression-oriented language designed for processing
		observability data (logs and metrics) in a safe and performant manner.

		This transform also implements an additional `errors` output. When the
		`drop_on_error` configuration value is set to `true`, events that result in
		runtime errors will be dropped from the default output stream and sent to
		the `errors` output instead. For a transform component named `foo`, this
		error output can be accessed by specifying `foo.errors` as the input to
		another component.

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

				Required if `file` is missing.
				"""
			common:      true
			required:    false
			type: string: {
				examples: [
					"""
						. = parse_json!(.message)
						.new_field = "new value"
						.status = to_int!(.status)
						.duration = parse_duration!(.duration, "s")
						.new_name = del(.old_name)
						""",
				]
				syntax:  "remap_program"
				default: null
			}
		}
		file: {
			description: """
				File path to the [Vector Remap Language](\(urls.vrl_reference)) (VRL) program to execute for each event.

				If a relative path is provided, its root is the current working directory.

				Required if `source` is missing.
				"""
			common:      true
			required:    false
			type: string: {
				examples: [
					"./my/program.vrl",
				]
				syntax:  "literal"
				default: null
			}
		}
		drop_on_error: {
			common:   false
			required: false
			description: """
				Drop the event from the primary output stream if the VRL program returns
				an error at runtime. These events will instead be written to the
				`errors` output.
				"""
			type: bool: default: false
		}
		drop_on_abort: {
			common:   false
			required: false
			description: """
				Drop the event if the VRL program is manually aborted through the `abort` statement.
				"""
			type: bool: default: true
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
		event_data_model: {
			title: "Event Data Model"
			body:  """
				You can use the `remap` transform with both log and metric events.

				Log events in the `remap` transform correspond directly to Vector's [log schema](\(urls.vector_log)),
				which means that the transform has access to the whole event.

				With metric events the remap transform has:

				* read-only access to the event's`.type`
				* read/write access to `kind`, but it can only be set to one of `incremental` or `absolute` and cannot be deleted
				* read/write access to `name`, but it cannot be deleted
				* read/write/delete access to `namespace`, `timestamp`, and keys in `tags`
				"""
		}
		lazy_event_mutation: {
			title: "Lazy Event Mutation"
			body:  #"""
				When you make changes to an event through VRL's path assignment syntax, the change
				isn't immediately applied to the actual event. If the program fails to run to
				completion, any changes made until that point are dropped and the event is kept in
				its original state.

				If you want to make sure your event is changed as expected, you have to rewrite
				your program to never fail at runtime (the compiler can help you with this).

				Alternatively, if you want to ignore/drop events that caused the program to fail,
				you can set the `drop_on_error` configuration value to `true`.

				Learn more about runtime errors in the [Vector Remap Language
				reference](\#(urls.vrl_runtime_errors)).
				"""#
		}
		emitting_multiple_events: {
			title: "Emitting multiple log events"
			body: #"""
				Multiple log events can be emitted from remap by assigning an array to the root path
				`.`. One log event is emitted for each input element of the array.

				If any of the array elements isn't an object, a log event is created that uses the
				element's value as the `message` key. For example, `123` is emitted as:

				```json
				{
				  "message": 123
				}
				```
				"""#
		}
	}

	telemetry: metrics: {
		processing_errors_total: components.sources.internal_metrics.output.metrics.processing_errors_total
	}
}
