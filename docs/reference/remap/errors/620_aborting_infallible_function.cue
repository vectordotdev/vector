package metadata

remap: errors: "620": {
	title: "Aborting infallible function"
	description: """
		You've specified that a function should abort on error even though the function is infallible.
		"""

	rationale: """
		In VRL, [infallible](\(urls.vrl_error_handling)) functions—functions that can't fail—don't require error
		handling, which in turn means it doesn't make sense to abort on failure using a `!` in the function call.
		"""

	resolution: """
		Remove the `!` from the function call.
		"""

	examples: [
		{
			"title": "\(title)"
			source: #"""
				encode_json!(["one", "two", "three"])
				"""#
			diff: #"""
				- 	encode_json!(["one", "two", "three"])
				+# 	encode_json(["one", "two", "three"])
				"""#
		},
	]
}
