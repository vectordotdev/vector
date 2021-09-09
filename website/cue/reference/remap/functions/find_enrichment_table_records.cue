package metadata

remap: functions: find_enrichment_table_records: {
	category: "Enrichment"
	description: """
		Searches an enrichment table for a row that matches the given
		condition.

		The condition is specified as an object of field to value. The
		given fields are searched with the enrichment table to find the
		rows that match the given values. All fields must match.

		There are currently two form of search criteria:

		1.  An exact match search. The given field must match the value
		exactly (case sensitivity can be specified with a separate parameter
		to the function.

		2. Date range search. The given field must be greater than or
		equal to the `from` date and less than or equal to the `to` date.
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
		{
			name:        "case_sensitive"
			description: "Should text fields match case exactly."
			required:    false
			type: ["boolean"]
			default:     true
		},
	]
	internal_failure_reasons: []
	return: types: ["object"]

	examples: [
		{
			title: "Exact match"
			source: #"""
				find_enrichment_table_records("csvfile",
																			{ "surname": "smith",
																				"firstname": "John" },
																			case_sensitive: false)
				"""#
			return: true
		},
		{
			title: "Date range search"
			source: #"""
				find_enrichment_table_records("csvfile",
																			{ "surname": "Smith",
																				"date_of_birth": { "from": t'1985-01-01',
																													 "to": t'1985-31-12'} })
				"""#
			return: true
		},
	]
}
