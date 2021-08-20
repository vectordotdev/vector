package metadata

remap: functions: get_enrichment_table_record: {
	category: "String"
	description: """
		Searches an enrichment table for a row that matches the given
		condition.

		The condition is specified as an object of field to value. The
		given fields are searched with the enrichment table to find the
		row that matches the given values. All fields must match. The
		search is case insensitive.
		"""

	arguments: [
		{
			name:        "table"
			description: "The enrichment table to search."
			required:    true
			type: ["string"]
		},
		{
			name:        "condition"
			description: "The condition to search on."
			required:    true
			type: ["object"]
		},
	]
	internal_failure_reasons: ["The row is not found."
	                           "Multiple rows are found that match the condition"
	]
	return: types: ["object"]

	examples: [
		{
			title: ""
			source: #"""
				get_enrichment_table_record("csvfile", { "surname": "smith", "firstname": "John" })
				"""#
			return: true
		},
	]
}
