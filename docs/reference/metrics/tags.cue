package metadata

_tags: {
	component_kind: {
		description: "The component's kind (options are `source`, `sink`, or `transform`)."
		required:    true
		options: {
			sink:      "Sink component."
			source:    "Source component."
			transform: "Transform component."
		}
	}
	component_name: {
		description: "The name of the component as specified in the Vector configuration."
		required:    true
		examples: ["file_source", "splunk_sink"]
	}
	component_type: {
		description: "The type of component (source, transform, or sink)."
		required:    true
		examples: ["file", "http", "honeycomb", "splunk_hec"]
	}
	instance: {
		description: "The Vector instance identified by host and port."
		required:    true
		examples: [_values.instance]
	}
	job: {
		description: "The name of the job producing Vector metrics."
		required:    true
		default:     "vector"
	}
}
