package metadata

remap: errors: "601": {
	title:       "Invalid timestamp"
	description: """
		The provided [timestamp literal](\(urls.vrl_expressions)#timestamp) is properly
		formed (i.e. it uses `t'...'` syntax) but the timestamp doesn't adhere to [RFC 3339](\(urls.rfc_3339)) format.
		"""

	rationale: "Invalid timestamps don't compile."

	resolution: """
		Bring the timestamp in conformance with [RFC 3339](\(urls.rfc_3339)) format.
		"""

	examples: [
		{
			"title": "\(title) formatting"
			source: #"""
				.timestamp = format_timestamp!(t'next Tuesday', format: "%v %R")
				"""#
			diff: #"""
				-.timestamp = format_timestamp!(t'next Tuesday', format: "%v %R")
				+.timestamp = format_timestamp!(t'2021-03-09T16:33:02.405806Z', format: "%v %R")
				"""#
		},
	]
}
