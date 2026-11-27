package metadata

components: transforms: "remap": {
	title: "Remap"

	description: """
		This is the recommended transform for parsing, shaping, and transforming data in Vector. It implements the
		[Vector Remap Language](\(urls.vrl_reference)) (VRL), an expression-oriented language designed for processing
		observability data (logs and metrics) in a safe and performant manner.

		Please refer to the [VRL reference](\(urls.vrl_reference)) when writing VRL scripts.
		"""

	classes: {
		development:   "stable"
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
	}

	configuration: generated.components.transforms.remap.configuration

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

	output: {
		logs: "": {
			description: "The modified input `log` event."
		}
		metrics: "": {
			description: "The modified input `metric` event."
		}
		traces: "": {
			description: "The modified input `trace` event."
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

				Field | Access | Details
				:-----|:-------|:------------------------------
				`type` | Read/write | You can set `type` to either `counter` or `gauge`, but only on a metric that is currently a counter or a gauge. The value and kind are kept.
				`kind` | Read/write | You can set `kind` to either `incremental` or `absolute` but not to an arbitrary value.
				`name` | Read/write |
				`timestamp` | Read/write/delete | You can only assign a valid [VRL timestamp](\(urls.vrl_expressions)/#timestamp) value, not a [VRL string](\(urls.vrl_expressions)/#string).
				`interval_ms` | Read/write/delete | You can assign it a non zero 32 byte unsigned interval
				`namespace` | Read/write/delete |
				`tags` | Read/write/delete | The `tags` field must be a [VRL object](\(urls.vrl_expressions)/#object) in which all keys and values are strings.
				`counter`, `gauge` | Read | The whole object that holds the value of the metric's current type.
				`counter.value`, `gauge.value` | Read/write | You can assign an integer or a float, but only under the metric's current type.
				`set` | Read | `.set.values`, an array of strings.
				`distribution` | Read | `.distribution.samples`, an array of `{value, rate}` objects, and `.distribution.statistic`, either `histogram` or `summary`.
				`aggregated_histogram` | Read | `.aggregated_histogram.buckets`, an array of `{upper_limit, count}` objects, plus `.aggregated_histogram.count` and `.aggregated_histogram.sum`. `sum` is left out when it isn't set, so it reads as `null` and `exists(.aggregated_histogram.sum)` is `false`.
				`aggregated_summary` | Read | `.aggregated_summary.quantiles`, an array of `{quantile, value}` objects, plus `.aggregated_summary.count` and `.aggregated_summary.sum`.
				`sketch` | Read | Summary statistics only: `.sketch.count`, `.sketch.min`, `.sketch.max`, `.sketch.sum`, and `.sketch.avg`.

				It's important to note that if you try to perform a disallowed action, such as deleting the `type`
				field using `del(.type)`, Vector doesn't abort the VRL program or throw an error. Instead, it ignores
				the disallowed action.
				"""
		}
		metric_values: {
			title: "Metric Values"
			body: """
				A metric's value is available only under the field for its current type. For example, `.gauge.value`
				holds the value of a gauge, and `.counter` is `null` for a gauge. Reading the whole event with `.` also
				includes the value field.

				The field names in the Event Data Model table match the JSON encoding of metrics that `vector tap`
				shows. For some metric types, these names differ from the value of `type`. For example, an aggregated
				histogram reports `aggregated histogram` in `.type` but exposes its count at
				`.aggregated_histogram.count`.

				You can assign an integer or a float to `.counter.value` or `.gauge.value` when it matches the metric's
				current type, for example, `.gauge.value = 100`. The value is stored as a float, so it reads back as
				`100.0`, and integers larger than `9007199254740992` (2^53) in absolute value might be rounded.
				Assigning anything else, such as a string, the field of another type, or a whole object like `.gauge`,
				leaves the value unchanged and the program keeps running. The other value fields are read-only, and
				`del()` on any value field returns `null` and leaves the value unchanged. To convert between counters
				and gauges, set `type`.

				Two kinds of numbers need special handling in VRL:

				* VRL floats can't represent NaN, so a NaN value reads as the string `"NaN"`. Infinities read as
				  floats, but `encode_json` writes them as `null`.
				* VRL integers are signed 64-bit, so a count larger than `9223372036854775807` reads as a string with
				  the exact decimal value.

				The type of a metric's value is known only at runtime, so use `float!` to assert that it's a float before
				using it in arithmetic, for example, `float!(.gauge.value) * 100`. `float!` fails on a NaN value because
				NaN reads as the string `"NaN"`.

				`unnest` copies the rest of the event into each row. On a metric event, the rest of the event includes the
				value. With `metric_tag_values: full`, `.tags.region` is an array, so on a distribution
				`unnest!(.tags.region)` copies every sample into every row. If the program only needs the tags, unnest a
				local copy instead, for example, `tag_view = {"tags": .tags}; rows = unnest!(tag_view.tags.region)`.

				Counter, gauge, and sample values are floats. The `==` operator treats `123` and `123.0` as equal, but
				`assert_eq!` doesn't, so use float literals in assertions, for example, `assert_eq!(.gauge.value, 123.0)`.
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
