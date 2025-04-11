package metadata

base: components: transforms: window: configuration: {
	flush_when: {
		description: """
			A condition used to flush the events.

			If the condition resolves to `true` for an event, the whole window is immediately flushed,
			including the event itself, and any following events if `num_events_after` is more than zero.
			"""
		required: true
		type: condition: {}
	}
	forward_when: {
		description: """
			A condition used to pass events through the transform without buffering.

			If the condition resolves to `true` for an event, the event is immediately forwarded without
			buffering and without preserving the original order of events. Use with caution if the sink
			cannot handle out of order events.
			"""
		required: false
		type: condition: {}
	}
	num_events_after: {
		description: "The maximum number of events to keep after the event matched by the `flush_when` condition."
		required:    false
		type: uint: default: 0
	}
	num_events_before: {
		description: "The maximum number of events to keep before the event matched by the `flush_when` condition."
		required:    false
		type: uint: default: 100
	}
}
