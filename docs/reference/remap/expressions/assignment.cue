package metadata

remap: expressions: assignment: {
	title: "Assignment"
	description: """
		An _assignment_ expression assigns the result of the right-hand-side expression to the left-hand-side
		target (path or variable).
		"""
	return: """
		Returns the value of the right-hand-side expression only if the expression succeeds. If the expression errors,
		the error must be [handled](\(urls.vrl_errors_reference)) and null is returned.
		"""

	grammar: {
		source: """
			target ~ ("," ~ error)? ~ operator ~ expression
			"""
		definitions: {
			target: {
				description: """
					The `target` must be a path,
					with an optional second variable for error handling if the right-hand side is fallible.
					"""
			}
			error: {
				description: """
					The `error` allows for optional assignment to errors when the right-hand-side expression is
					fallible. This is commonly used when invoking fallible functions.
					"""
			}
			operator: {
				description: """
					The `operator` delimits the `target` and `expression` and defines assignment conditions.
					"""
				enum: {
					"=": """
						Simple assignment operator. Assigns the result from the right-hand side to the left-hand side:

						```vrl
						.field = "value"
						```
						"""
					"??=": """
						Assigns _only_ if the right-hand side doesn't error. This is useful when invoking fallible
						functions on the right-hand side:

						```vrl
						.structured ??= parse_json(.message)
						```
						"""
				}
			}
			expression: {
				description: """
					If the `target` is a variable, the `expression` can be any expression.

					If the `target` is a path, the `expression` can be any expression that returns a supported object
					value type (i.e. not a regular expression).
					"""
			}
		}
	}

	examples: [
		{
			title: "Path assignment"
			source: #"""
				.message = "Hello, World!"
				"""#
			return: "Hello, World!"
			output: log: message: "Hello, World!"
		},
		{
			title: "Nested path assignment"
			source: #"""
				.parent.child = "Hello, World!"
				"""#
			return: "Hello, World!"
			output: log: parent: child: "Hello, World!"
		},
		{
			title: "Double assignment"
			source: #"""
				.first = .second = "Hello, World!"
				"""#
			return: "Hello, World!"
			output: log: {
				first:  "Hello, World!"
				second: "Hello, World!"
			}
		},
		{
			title: "Array element assignment"
			source: #"""
				.array[1] = "Hello, World!"
				"""#
			return: "Hello, World!"
			output: log: array: [null, "Hello, World!"]
		},
		{
			title: "Variable assignment"
			source: #"""
				my_variable = "Hello, World!"
				"""#
			return: "Hello, World!"
		},
		{
			title: "Fallible assignment (success)"
			source: #"""
				.parsed, .err = parse_json("{\"Hello\": \"World!\"}")
				"""#
			output: log: {
				parsed: {"Hello": "World"}
				err: null
			}
		},
		{
			title: "Fallible assignment (error)"
			source: #"""
				.parsed, .err = parse_json("malformed")
				"""#
			output: log: {
				parsed: null
				err:    #"function call error for "parse_json" at (14:37): unable to parse json: expected value at line 1 column 1"#
			}
		},
	]
}
