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
		requirements: []
		warnings: []
		notices: []
	}

	configuration: base.components.transforms.remap.configuration

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
		traces: true
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
				You can use the remap transform to handle all event types (log, metric, trace).
				The `remap` transform cannot convert from one event type to another, the input type must match the output type.
				For example, a log event can only be remap-ed and outputted as a log event and cannot be converted to a metric.

				For converting from one type to another, please see [Log to Metric](\(urls.vector_log_to_metric_transform)) and
				[Metric to Log](\(urls.vector_metric_to_log_transform)) transforms.

				Log events in the `remap` transform correspond directly to Vector's [log schema](\(urls.vector_log)),
				which means that the transform has access to the whole event and no restrictions on how the event can be
				modified.

				With [metric events](\(urls.vector_metric)), VRL is much more restrictive. Below is a field-by-field
				breakdown of VRL's access to metrics:

				Field | Access | Specific restrictions (if any)
				:-----|:-------|:------------------------------
				`type` | Read only |
				`kind` | Read/write | You can set `kind` to either `incremental` or `absolute` but not to an arbitrary value.
				`name` | Read/write |
				`timestamp` | Read/write/delete | You assign only a valid [VRL timestamp](\(urls.vrl_expressions)/#timestamp) value, not a [VRL string](\(urls.vrl_expressions)/#string).
				`namespace` | Read/write/delete |
				`tags` | Read/write/delete | The `tags` field must be a [VRL object](\(urls.vrl_expressions)/#object) in which all keys and values are strings.

				It's important to note that if you try to perform a disallowed action, such as deleting the `type`
				field using `del(.type)`, Vector doesn't abort the VRL program or throw an error. Instead, it ignores
				the disallowed action.
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

	outputs: [
		components._default_output,
		{
			name: "dropped"
			description: """
				This transform also implements an additional `dropped` output. When the
				`drop_on_error` or `drop_on_abort` configuration values are set to `true`
				and `reroute_dropped` is also set to `true`, events that result in runtime
				errors or aborts will be dropped from the default output stream and sent to
				the `dropped` output instead. For a transform component named `foo`, this
				output can be accessed by specifying `foo.dropped` as the input to another
				component. Events sent to this output will be in their original form,
				omitting any partial modification that took place before the error or abort.
				"""
		},
	]
}
