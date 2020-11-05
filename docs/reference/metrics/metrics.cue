package metadata

_metrics: {
	vector_events_processed_total: {
		type:        "counter"
		description: "The total number of events processed by this component."
		tags:        _component_tags
	}
	vector_processed_bytes_total: {
		description: "The total number of bytes processed by the component."
		type:        "counter"
		tags:        _component_tags
	}

	// Helpers
	_component_tags: {
		component_kind: _tags.component_kind
		component_name: _tags.component_name
		component_type: _tags.component_type
		instance:       _tags.instance
		job:            _tags.job
	}
}
