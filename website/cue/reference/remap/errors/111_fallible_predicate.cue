package metadata

remap: errors: "111": {
	title:       "Unhandled predicate error"
	description: """
		A predicate is fallible and its [runtime error](\(urls.vrl_runtime_errors)) isn't handled in the VRL
		program.
		"""
	rationale:   remap._fail_safe_blurb
	resolution:  """
		[Handle](\(urls.vrl_error_handling)) the runtime error by [assigning](\(urls.vrl_error_handling_assigning)),
		[coalescing](\(urls.vrl_error_handling_coalescing)), or [raising](\(urls.vrl_error_handling_raising)) the
		error.
		"""

	examples: [
		{
			"title": "\(title) (predicate)"
			source: #"""
				if contains(.field, "thing") {
				  log("thing")
				}
				"""#
			diff: #"""
				-       if contains(.field, "thing") {
				+#      if contains(.field, "thing") ?? false {
				"""#
		},
	]
}
