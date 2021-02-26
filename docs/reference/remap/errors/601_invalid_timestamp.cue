package metadata

remap: errors: "601": {
	title:       "Invalid timestamp"
	description: """
		The provided [timestamp literal](\(urls.vrl_expressions)#\(remap.literals.timestamp.anchor)) is properly
		formed (i.e. it uses `t'...'` syntax) but the timestamp doesn't adhere to [RFC 3339](\(urls.rfc_3339)) format.
		"""

	rationale: "Invalid timestamps don't compile."

	resolution: """
		Bring the timestamp in conformance with [RFC 3339](\(urls.rfc_3339)) format.
		"""

	examples: [
		{
			"title": "\(title) (parsing)"
			source: #"""
				parse_timestamp!("next Tuesday", format: "%v %R")
				"""#
			diff: #"""
				- 	parse_timestamp!("next Tuesday", format: "%v %R")
				+# 	parse_timestamp!("10-Oct-2020 16:00", format: "%v %R")
				"""#
		},
	]
}
