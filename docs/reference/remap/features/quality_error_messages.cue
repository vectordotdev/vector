remap: features: quality_error_messages: {
	title: "Quality error messages"
	description: """
		VRL strives to provide high-quality, helpful error messages, reducing the development iteration	cycle.
		For example:

		```
		error: program aborted
		  ┌─ :2:1
		  │
		2 │ parse_json!(1)
		  │ ^^^^^^^^^^^^^^
		  │ │
		  │ function call error
		  │ unable to parse json: key must be a string at line 1 column 3
		  │
		  = see function documentation at: https://master.vector.dev/docs/reference/remap/#parse_json
		  = see language documentation at: https://vector.dev/docs/reference/vrl/
		```
		"""

	principles: {
		performance: false
		safety:      false
	}
}
