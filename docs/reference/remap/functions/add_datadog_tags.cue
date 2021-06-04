package metadata

remap: functions: add_datadog_tags: {
	category: "String"
	description: #"""
		Add a set of tags to an existing tags list following the Datadog tag format. Datadog logs tags should be kept in the `ddtags` fields to be properly accounted for if the
		event is ultimately sent to Datadog using the `datadog_logs` sink. Duplicated tags are removed.
		"""#

	arguments: [
		{
			name:        "value"
			description: "The initial list of tags"
			required:    true
			type: ["array"]
		},
		{
			name:        "tags"
			description: "The tags to be added."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["string"]
	}

	examples: [
		{
			title: "Simple tag addition"
			source: #"""
				add_datadog_tags!("env:beta,platform:linux", ["via:vector", "aggregated:true"])
				"""#
			return: "aggregated:true,env:beta,platform:linux,via:vector"
		},
		{
			title: "Duplicated tags only appear once in the returned value"
			source: #"""
				add_datadog_tags!("env:beta,platform:linux", ["via:vector", "env:beta"])
				"""#
			return: "env:beta,platform:linux,via:vector"
		},
	]
}
