package metadata

remap: functions: remove_datadog_tags: {
	category: "String"
	description: #"""
		Remove a set of tags from an existing tags list that follows the Datadog tag format. Datadog logs tags should be kept in the `ddtags` fields to be properly accounted for if the
		event is ultimately sent to Datadog using the `datadog_logs` sink. Tags are usually following the `key:value` convention thus the `remove_datadog_tags` can be used to remove all
		tags matching a given `key` or an exact `key:value` tag.
		"""#

	arguments: [
		{
			name:        "tags"
			description: "The initial list of tags"
			required:    true
			type: ["array"]
		},
		{
			name:        "key"
			description: "The key of the tag(s) that will be removed."
			required:    true
			type: ["string"]
		},
		{
			name:        "value"
			description: "The value of the tag that will be removed."
			required:    false
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["string"]
	}

	examples: [
		{
			title: "Exact match removal"
			source: #"""
				remove_datadog_tags!("env:beta,env:local,platform:windows", "env", "beta")
				"""#
			return: "env:local,platform:windows"
		},
		{
			title: "Matching on key can remove multiple tags"
			source: #"""
				remove_datadog_tags!("env:beta,env:local,platform:windows", "env")
				"""#
			return: "platform:windows"
		},
	]
}
