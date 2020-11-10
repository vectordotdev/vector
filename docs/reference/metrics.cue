package metadata

// All available metrics
_vector_events_processed_total: {
	type:        "counter"
	description: "The total number of events processed by this component."
	tags:        _component_tags
}
_vector_processed_bytes_total: {
	description: "The total number of bytes processed by the component."
	type:        "counter"
	tags:        _component_tags
}
_vector_processing_errors_total: {
	description: "The total number of processing errors encountered by the component."
	type:        "counter"
	tags:        _component_tags & {
		error_type: _error_type
	}
}

// Convenient groupings of tags
_component_tags: {
	component_kind: _component_kind
	component_name: _component_name
	component_type: _component_type
	instance:       _instance
	job:            _job
}

// All available tags
_component_kind: {
	description: "The component's kind (options are `source`, `sink`, or `transform`)."
	required:    true
	options: {
		sink:      "Sink component."
		source:    "Source component."
		transform: "Transform component."
	}
}
_component_name: {
	description: "The name of the component as specified in the Vector configuration."
	required:    true
	examples: ["file_source", "splunk_sink"]
}
_component_type: {
	description: "The type of component (source, transform, or sink)."
	required:    true
	examples: ["file", "http", "honeycomb", "splunk_hec"]
}
_error_type: {
	description: "The type of the error"
	required:    true
	options: [
		"field_missing",
		"invalid_metric",
		"mapping_failed",
		"match_failed",
		"parse_failed",
		"render_error",
		"type_conversion_failed",
		"value_invalid",
	]
}
_instance: {
	description: "The Vector instance identified by host and port."
	required:    true
	examples: [_values.instance]
}
_job: {
	description: "The name of the job producing Vector metrics."
	required:    true
	default:     "vector"
}
