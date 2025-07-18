package metadata

remap: expressions: function_call: {
	title:       "Function call"
	description: """
		A _function call_ expression invokes built-in [VRL functions](\(urls.vrl_functions)).
		"""
	return:      """
		Returns the value of the function invocation if the invocation succeeds. If the invocation fails, the error must
		be [handled](\(urls.vrl_errors_reference)) and null is returned.

		Functions can _only_ return a single value. If multiple values are relevant, you should wrap them in a data
		structure fit to hold them, such as an array or object (note that VRL doesn't support tuples).
		"""

	grammar: {
		source: """
			function ~ abort? ~ "(" ~ arguments? ~ ")" ~ closure?
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

					```coffee
					result = f!()
					```

					Otherwise, errors must be handled:

					```coffee
					result, err = f()
					```

					Failure to handle errors from fallible functions results in compile-time errors. See the
					[error reference](\(urls.vrl_errors_reference)) for more info.
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

							```coffee
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

							```coffee
							f(1, 2)
							```
							"""
					}
					type_safety: {
						title:       "Argument type safety"
						description: """
							Function arguments enforce type safety when the type of the value supplied is known:

							```coffee
							round("not a number") # fails at compile time
							```

							If the type of the value is not known, you need to handle the potential argument error:

							```coffee
							number = int(.message) ?? 0
							round(number)
							```

							See the [errors reference](\(urls.vrl_errors_reference)) for more info.
							"""
					}
				}
			}
			closure: {
				description: """
					The `closure` is an optional piece of code resolved by the function call. It is primarily used in functions that iterate over collections. Its syntax is as follows:

					```coffee
					for_each([]) -> |index, value| { ... }
					```
					"""
			}
		}
	}

	characteristics: {
		fallibility: {
			title: "Function Fallibility"
			description: """
				VRL functions can be marked as "fallible" or
				"infallible". When a function is defined as
				fallible, it can fail at runtime, requiring the
				error to be handled before the program can be
				compiled.

				If a function is defined as infallible, it means
				that **given the correct function arguments**,
				the function can never fail at runtime, and thus
				no error handling is needed.

				Note that even if a function is defined as
				infallible, if any of its arguments can fail at
				runtime, the function is considered to be
				fallible, and thus the error case needs to be
				handled in this case.

				The VRL compiler ensures all potential errors in
				a program are handled, so there's no need to
				worry about missing any potential runtime
				failures.
				"""
		}
		deprecation: {
			title: "Deprecation"
			description: """
				VRL functions can be marked as "deprecated". When a function
				is deprecated, a warning will be shown at runtime.

				Suggestions on how to update the VRL program can usually be found in the actual warning and the function documentation.
				"""
		}
		purity: {
			title: "Purity"
			description: """
				VRL functions can be marked as "pure" or "impure". When a function
				is pure, it is idempotent and has no side-effects. Otherwise, it is impure.
				"""
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
		{
			title: "Infallible function invocation"
			source: #"""
				split("apples and pears and bananas", " and ")
				"""#
			return: ["apples", "pears", "bananas"]
		},
		{
			title: "Fallible function invocation"
			input: log: message: "apples and pears and bananas"
			source: #"""
				# The compiler cannot determine the argument type thus we need to do error handling.
				split!(.message, " and ")
				"""#
			return: ["apples", "pears", "bananas"]
		},
	]
}
