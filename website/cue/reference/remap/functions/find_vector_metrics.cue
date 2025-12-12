package metadata

remap: functions: find_vector_metric: {
	category:    "Metrics"
	description: """
		Searches internal Vector metrics by name and optionally by tags. Returns all matching
		metrics.

		\(remap._vector_metrics_explainer)
		"""

	arguments: [
		{
			name:        "key"
			description: "The metric name to search."
			required:    true
			type: ["string"]
		},
		{
			name: "tags"
			description: """
				Tags to filter the results on. Values in this object support wildcards ('*') to
				match on parts of the tag value.
				"""
			required: false
			type: ["object"]
		},
	]
	internal_failure_reasons: []
	return: types: ["array"]

	examples: [
		{
			title: "Find vector internal metrics matching the name"
			source: #"""
				find_vector_metric!("utilization")
				"""#
			return: [{"name": "utilization", "tags": {}, "type": "gauge", "kind": "absolute", "value": 0.5}]
		},
		{
			title: "Find vector internal metrics matching the name and tags"
			source: #"""
				find_vector_metric!("utilization", tags: {"component_id": "test"})
				"""#
			return: [{"name": "utilization", "tags": {"component_id": ["test"]}, "type": "gauge", "kind": "absolute", "value": 0.5}]
		},
	]
}
