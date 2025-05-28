package metadata

remap: functions: find_enrichment_table_records: {
	category:    "Enrichment"
	description: """
		Searches an [enrichment table](\(urls.enrichment_tables_concept)) for rows that match the
		provided condition.

		\(remap._enrichment_table_explainer)
		"""

	arguments: [
		{
			name:        "table"
			description: "The [enrichment table](\(urls.enrichment_tables_concept)) to search."
			required:    true
			type: ["string"]
		},
		{
			name: "condition"
			description: """
				The condition to search on. Since the condition is used at boot time to create
				indices into the data, these conditions must be statically defined.
				"""
			required: true
			type: ["object"]
		},
		{
			name: "select"
			description: """
				A subset of fields from the enrichment table to return. If not specified,
				all fields are returned.
				"""
			required: false
			type: ["array"]
		},
		{
			name:        "case_sensitive"
			description: "Whether text fields need to match cases exactly."
			required:    false
			type: ["boolean"]
			default: true
		},
	]
	internal_failure_reasons: []
	return: types: ["array"]

	examples: [
		{
			title: "Exact match"
			source: #"""
				find_enrichment_table_records!("test",
				  {
					"surname": "smith",
				  },
				  case_sensitive: false)
				"""#
			return: [{"id": 1, "firstname": "Bob", "surname": "Smith"},
					{"id":          2, "firstname":   "Fred", "surname": "Smith"},
			]
		},
		{
			title: "Date range search"
			source: #"""
				find_enrichment_table_records!("test",
				  {
					"surname": "Smith",
					"date_of_birth": {
					  "from": t'1985-01-01T00:00:00Z',
					  "to": t'1985-12-31T00:00:00Z'
					}
				  })
				"""#
			return: [{"id": 1, "firstname": "Bob", "surname": "Smith"},
					{"id":          2, "firstname":   "Fred", "surname": "Smith"},
			]
		},
	]
}
