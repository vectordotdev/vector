package metadata

remap: expressions: function_call: {
	title: "Function call"
	description: """
		A _function call_ expression invokes built-in VRL functions.
		"""
	return: """
		Returns the value of the function invocation if the invocation succeeds. If the invocation fails, then the
		error must be [handled](\(urls.vrl_errors_reference)) and null is returned.

		Functions can _only_ return a single value. If multiple values are relevant, they will be wrapped in a data
		structure fit to hold them, such as an array or map.
		"""

	grammar: {
		source: """
			function ~ abort? ~ "(" ~ arguments? ~ ")"
			"""
		definitions: {
			function: {
				description: """
					`function` represents the name of the built-in function.
					"""
			}
			abort: {
				description: """
					`abort` represents a literal `!` that can optionally be used with fallible functions to abort
					the program when the function fails:

					```vrl
					result = f!()
					```

					Otherwise, errors must be handled:

					```vrl
					result, err = f()
					```

					Failure to handle errors from fallible function will result in compile-time errors. Please see the
					[error reference](\(urls.vrl_errors_reference)).
					"""
			}
			arguments: {
				description: """
					The `arguments` are comma-delimited expressions that can optionally	be prefixed with the
					documented name.
					"""

				characteristics: {
					named: {
						title: "Named arguments"
						description: """
							_All_ function arguments in VRL are assigned names, including required leading arguments.
							Named arguments are suffixed with a colon (`:`), with the value proceeding the name:

							```vrl
							argument_name: "value"
							argument_name: (1 + 2)
							```

							The value is treated as another expression.
							"""
					}
					positional: {
						title: "Positional arguments"
						description: """
							Function calls support nameless positional arguments. Arguments must be supplied in the order
							they are documented:

							```vrl
							f(1, 2)
							```
							"""
					}
					type_safety: {
						title:       "Argument type safety"
						description: """
							Function arguments enforce type safety when the type of the value supplied is known:

							```vrl
							number = round("not a number") # fails at compile time
							```

							If the type of the value is not known, you are required to handle the potential argument
							error:

							```vrl
							number, err = round(.message)
							```

							See the [errors reference](\(urls.vrl_errors_reference)) for more info.
							"""
					}
				}
			}
		}
	}

	examples: [
		{
			title: "Positional function invocation"
			source: #"""
				split("hello, world!", ", ")
				"""#
			return: ["hello", "world!"]
		},
		{
			title: "Named function invocation (ordered)"
			source: #"""
				split("hello, world!", pattern: ", ")
				"""#
			return: ["hello", "world!"]
		},
		{
			title: "Named function invocation (unordered)"
			source: #"""
				split(pattern: ", ", value: "hello, world!")
				"""#
			return: ["hello", "world!"]
		},
	]
}
