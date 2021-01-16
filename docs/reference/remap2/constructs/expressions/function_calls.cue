package metadata

remap2: constructs: expressions: constructs: function_calls: {
	title: "Function calls"
	description:	"""
		A function call expression invokes built-in VRL functions.
		"""

	examples: [
		"f(1, 2)",
		"f!(1, 2)",
		"f(name1: 1, name2: 2)",
		"f(1, name2: 2)",
		""
	]

	characteristics: {
		arguments: {
			title: "Function arguments"
			description:	"""
				Functions can optionally take arguments. Arguments are comma-delimited expressions that can optionally
				be specified with a name. The following invocations are functionally equivalent:

				```js
				f("value1", "value2")
				f(name1: "value1", name2: "value2")
				f("value1", name2: "value2")
				f(name2: "value2", name1: "value1")
				```
				"""

			characteristics: {
				named: {
					title: "Named arguments"
					description:	"""
						_All_ function arguments in VRL are assigned names, including required leading arguments.
						Named arguments are suffixed with a colon (`:`), with the value proceeding the name:

						```
						argument_name: "value"
						argument_name: (1 + 2)
						```

						The value is treated as another expression.
						"""
				}
				positional: {
					title: "Positional arguments"
					description:	"""
						Function calls support nameless positional arguments. Arguments must be supplied in the order
						they are documented:

						```
						f(1, 2)
						```
						"""
				}
				type_safety: {
					title: "Type safety"
					description:	"""
						Function arguments enforce type safety when the type of the value supplied is known:

						```vrl
						number = round("not a number") # fails at compile time
						```

						If the type of the value is not known, you are required to handle the potential argument
						error:

						```vrl
						number, err = round(.message)
						```

						See the [errors reference](\(urls.vrl_errors_reference)). for more info.
						"""
				}
			}
		}
		custom: {
			title: "Custom functions"
			description:	"""
				VRL does _not_ currently support custom functions.
				"""
		}
		fallibility: {
			title: "Function fallibility"
			description:	"""
				Error-safety is a defining characteristic of VRL which is largely acheived by handling errors from
				_fallible_ functions. Fallibility is binary charactertic; a function is fallible or it is not. This
				characteristic is clearly documented for each function in the
				[function reference](\(urls.vrl_functions_reference)).

				If a function is _not_ fallible, then its invocation does not require error handling:

				```
				result = f()
				```

				If a function _is_ fallible, then its error must be handled in one of two ways:

				```
				result, err = f()
				result = f!()
				```

				Failure to handle the error will result in a compile-time error. Please see the
				[error reference](\(urls.vrl_errors_reference)).
				"""
		}
		returns: {
			title: "Function returns"
			description:	"""
				VRL functions can only return a _single_ value, multiple values will be contained in a data structure
				fit to hold them, such as an array or map.
				"""
		}
	}
}
