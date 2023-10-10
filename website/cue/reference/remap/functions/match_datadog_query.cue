package metadata

remap: functions: match_datadog_query: {
	category:    "Object"
	description: """
		Matches an object against a [Datadog Search Syntax](\(urls.datadog_search_syntax)) query.
		"""

	arguments: [
		{
			name:        "value"
			description: "The object."
			required:    true
			type: ["object"]
		},
		{
			name:        "query"
			description: "The Datadog Search Syntax query."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: types: ["boolean"]

	examples: [
		{
			title: "OR query"
			source: #"""
				match_datadog_query({"message": "contains this and that"}, "this OR that")
				"""#
			return: true
		},
		{
			title: "AND query"
			source: #"""
				match_datadog_query({"message": "contains only this"}, "this AND that")
				"""#
			return: false
		},
		{
			title: "Facet wildcard"
			source: #"""
				match_datadog_query({"custom": {"name": "foo"}}, "@name:foo*")
				"""#
			return: true
		},
		{
			title: "Tag range"
			source: #"""
				match_datadog_query({"tags": ["a:x", "b:y", "c:z"]}, s'b:["x" TO "z"]')
				"""#
			return: true
		},
	]
}
