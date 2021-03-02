package metadata

remap: errors: "110": {
	title:       "Invalid argument type"
	description: """
		An argument passed to a [function call expression](\(urls.vrl_expressions)#\(remap.literals.regular_expression.anchor))
		isn't a supported type.
		"""
	rationale:   """
		VRL is [type safe](\(urls.vrl_type_safety)) and requires that types align upon compilation. This provides
		important [safety guarantees](\(urls.vrl_safety)) to VRL and helps to ensure that VRL programs run reliably when
		deployed.
		"""
	resolution: #"""
		You must guarantee the type of the variable by using the appropriate [type](\(urls.vrl_functions)#type) or
		[coercion](\(urls.vrl_functions)#coerce) function.
		"""#

	examples: [...{
		source: #"""
			downcase(.message)
			"""#
	}]

	examples: [
		{
			"title": "\(title) (guard with defaults)"
			diff: #"""
				+.message = string(.message) ?? ""
				 downcase(.message)
				"""#
		},
		{
			"title": "\(title) (guard with errors)"
			diff: #"""
				 downcase(string!(.message))
				"""#
		},
		{
			"title": "\(title) (guard with if expressions)"
			diff: #"""
				+if is_string(.message) {
				 	downcase(.message)
				+ }
				"""#
		},
	]
}
