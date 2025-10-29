package metadata

remap: functions: aggregate_vector_metric: {
	category:    "Metrics"
	description: """
		Aggregates internal Vector metrics, using one of 4 aggregation functions, filtering by name
		and optionally by tags. Returns the aggregated value. Only includes counter and gauge metrics.

		\(remap._vector_metrics_explainer)
		"""

	arguments: [
		{
			name:        "function"
			description: "The metric name to search."
			required:    true
			type: ["string"]
			enum: {
				sum: "Sum the values of all the matched metrics"
				avg: "Find the average of the values of all the matched metrics"
				max: "Find the highest metric value of all the matched metrics"
				min: "Find the lowest metric value of all the matched metrics"
			}
		},
		{
			name:        "key"
			description: "The metric name to aggregate."
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
	return: types: ["float"]

	examples: [
		{
			title: "Sum vector internal metrics matching the name"
			source: #"""
				aggregate_vector_metrics("sum", "utilization")
				"""#
			return: 0.0
		},
		{

			title: "Sum vector internal metrics matching the name and tags"
			source: #"""
				aggregate_vector_metrics("sum", "utilization", tags: {"component_id": "test"})
				"""#
			return: 0.0
		},
		{
			title: "Average of vector internal metrics matching the name"
			source: #"""
				aggregate_vector_metrics("avg", "utilization")
				"""#
			return: 0.0
		},
		{
			title: "Max of vector internal metrics matching the name"
			source: #"""
				aggregate_vector_metrics("max", "utilization")
				"""#
			return: 0.0
		},
		{
			title: "Min of vector internal metrics matching the name"
			source: #"""
				aggregate_vector_metrics("max", "utilization")
				"""#
			return: 0.0
		},
	]
}
