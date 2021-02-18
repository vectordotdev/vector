package metadata

remap: errors: "601": {
	title:       "Invalid timestamp"
	description: """
		The provided [timestamp](\(urls.vrl_expressions)#\(remap.literals.timestamp.anchor)) is malformed.
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
			raises: compiletime: #"""
				function call error for "parse_timestamp" at (0:49): Invalid timestamp "next Tuesday": input contains invalid characters
				"""#
			diff: #"""
				- 	parse_timestamp!("next Tuesday", format: "%v %R")
				+# 	parse_timestamp!("10-Oct-2020 16:00", format: "%v %R")
				"""#
		},
	]
}
