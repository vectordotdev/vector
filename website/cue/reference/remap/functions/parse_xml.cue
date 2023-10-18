package metadata

remap: functions: parse_xml: {
	category: "Parse"
	description: """
		Parses the `value` as XML.
		"""
	notices: [
		"""
			Valid XML must contain exactly one root node. Always returns an object.
			""",
	]

	arguments: [
		{
			name:        "value"
			description: "The string representation of the XML document to parse."
			required:    true
			type: ["string"]
		},
		{
			name:        "include_attr"
			description: "Include XML tag attributes in the returned object."
			required:    false
			default:     true
			type: ["boolean"]
		},
		{
			name:        "attr_prefix"
			description: "String prefix to use for XML tag attribute keys."
			required:    false
			default:     "@"
			type: ["string"]
		},
		{
			name:        "text_key"
			description: "Key name to use for expanded text nodes."
			required:    false
			default:     "text"
			type: ["string"]
		},
		{
			name:        "always_use_text_key"
			description: "Always return text nodes as `{\"<text_key>\": \"value\"}.`"
			required:    false
			default:     false
			type: ["boolean"]
		},
		{
			name:        "parse_bool"
			description: "Parse \"true\" and \"false\" as boolean."
			required:    false
			default:     true
			type: ["boolean"]
		},
		{
			name:        "parse_null"
			description: "Parse \"null\" as null."
			required:    false
			default:     true
			type: ["boolean"]
		},
		{
			name:        "parse_number"
			description: "Parse numbers as integers/floats."
			required:    false
			default:     true
			type: ["boolean"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a valid XML document.",
	]
	return: types: ["object"]

	examples: [
		{
			title: "Parse XML"
			source: #"""
				value = s'<book category="CHILDREN"><title lang="en">Harry Potter</title><author>J K. Rowling</author><year>2005</year></book>';

				parse_xml!(value, text_key: "value", parse_number: false)
				"""#
			return: {
				"book": {
					"@category": "CHILDREN"
					"author":    "J K. Rowling"
					"title": {
						"@lang": "en"
						"value": "Harry Potter"
					}
					"year": "2005"
				}
			}
		},
	]
}
