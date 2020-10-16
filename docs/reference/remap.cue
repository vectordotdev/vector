package metadata

remap: {
	errors: {
		ArgumentError: {
			description: "Raised when the provided input is not a supported type."
		}
		ParseError: {
			description: "Raised when the provided input cannot be parsed."
		}
	}

	functions: {
		parse_json: {
			arguments: [
				{
					required: true
					type:     "string"
				},
			]
			category: "parse"
			description: #"""
				Returns an `object` whose text representation is a JSON
				payload in `string` form.

				`string` must be the string representation of a JSON
				payload. Otherwise, an `ParseError` will be raised.
				"""#
			examples: [
				{
					title: "Success"
					input: {
						message: #"{"key": "val"}"#
					}
					source: #"""
						message = del(.message)
						. = parse_json(message)
						"""#
					output: {
						key: "val"
					}
				},
				{
					title: "Error"
					input: {
						message: "malformed"
					}
					source: "parse_json(.message)"
					output: {
						error: errors.ParseError
					}
				},
			]
		}

		to_int: {
			arguments: [
				{
					required: true
					type:     "string"
				},
			]
			category: "coerce"
			description: #"""
				Returns an `integer` whose text representation is `string`.

				`string` must be the string representation of an `integer`.
				Otherwise, an `ArgumentError` will be raised.
				"""#
			examples: [
				{
					title: "Success"
					input: {
						integer: "2"
					}
					source: "to_int(.integer)"
					output: {
						integer: 2
					}
				},
				{
					title: "Error"
					input: {
						integer: "hi"
					}
					source: "to_int(.integer)"
					output: {
						error: errors.ArgumentError
					}
				},
			]
		}
	}
}
