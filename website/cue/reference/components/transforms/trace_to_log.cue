package metadata

components: transforms: trace_to_log: {
	title: "Trace to Log"

	description: """
		Converts a trace event into a log event. This preserves all trace
		attributes (span IDs, trace IDs, etc.) as log fields without modification.
		This transformation does not add any new fields, nor does it validate the
		output events are valid traces.
		"""

	classes: {
		commonly_used: false
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		convert: {}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: generated.components.transforms.trace_to_log.configuration

	input: {
		logs:    false
		metrics: null
		traces:  true
	}

	output: {
		logs: "": {
			description: "The converted `log` event."
		}
	}

	examples: [
		{
			title: "Trace to Log"

			configuration: {}

			input: [
				{
					trace: {
						span_id:   "abc123"
						trace_id:  "xyz789"
						span_name: "test-span"
						service:   "my-service"
					}
				},
			]

			output: [
				{
					log: {
						span_id:   "abc123"
						trace_id:  "xyz789"
						span_name: "test-span"
						service:   "my-service"
					}
				},
			]
		},
	]

	how_it_works: {
		conversion_behavior: {
			title: "Conversion Behavior"
			body: """
				The trace to log conversion is a straightforward transformation that takes all fields
				from the trace event and preserves them as fields in the resulting log event. This includes
				span IDs, trace IDs, span names, and any other trace attributes. The conversion does not modify
				or restructure the data, making it a simple pass-through with a type change from trace to log.
				"""
		}
	}
}
