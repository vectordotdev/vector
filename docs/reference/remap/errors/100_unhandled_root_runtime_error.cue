package metadata

remap: errors: "100": {
	title:       "Unhandled root runtime error"
	description: """
		A root expression is fallible and its [runtime error](\(urls.vrl_runtime_errors)) is not handled.
		"""
	rationale:   """
		VRL is [fail-safe](\(urls.vrl_fail_safety)) and requires that all possible runtime errors be handled. This
		contributes heavily to VRL's [safety principle](\(urls.vrl_safety)), ensuring that VRL programs are reliable
		once deployed.
		"""
	resolution:  """
		[Handle](\(urls.vrl_error_handling)) the runtime error by [assigning](\(urls.vrl_error_handling_assigning)),
		[coalescing](\(urls.vrl_error_handling_coalescing)), or [raising](\(urls.vrl_error_handling_raising)) the
		error.
		"""

	examples: [
		{
			"title": "\(title) (assigning)"
			source: #"""
				get_env_var("HOST")
				"""#
			raises: compiletime: #"""
				error: \#(title)
				  ┌─ :1:1
				  │
				1 │ 	(5 / 2)
				  │     ^^^^^
				  │     │
				  │     this expression is unhandled
				  │
				"""#
			diff: #"""
				- 	get_env_var("HOST")
				+# 	.host = get_env_var("HOST")
				"""#
		},
	]
}
