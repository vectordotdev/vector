package metadata

remap: functions: parse_cbor: {
	category:    "Parse"
	description: """
		Parses the `value` as [CBOR](\(urls.cbor)).
		"""
	notices: [
		"""
			Only CBOR types are returned.
			""",
	]

	arguments: [
		{
			name:        "value"
			description: "The CBOR payload to parse."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a valid CBOR-formatted payload.",
	]
	return: types: ["boolean", "integer", "float", "string", "object", "array", "null"]

	examples: [
		{
			title: "Parse CBOR"
			source: #"""
				parse_cbor!(decode_base64!("oWVmaWVsZGV2YWx1ZQ=="))
				"""#
			return: field: "value"
		},
	]
}
