remap: functions: to_bool: {
	arguments: [
		{
			name:        "value"
			description: "The value to convert to a Boolean."
			required:    true
			type: ["boolean", "integer", "float", "null", "string"]
		},
	]
	return: ["float"]
	category: "Coerce"
	description: #"""
		Converts the provided value to a Boolean. The conversion rules vary by type:

		Type    | Rule
		:-------|:----
		String  | These return `true`: `"true"`, `"t"`, `"yes"`, `"y"`. These return `false`: `"false"`, `"f"`, `"no"`, `"n"`, `"0"`.
		Float   | `0.0` returns `false`; all other floats return `true`
		Integer | `0` returns `false`; all other integers return `true`
		Null    | `null` always returns `false`
		Boolean | Returns the provided Boolean
		"""#
	examples: [
		{
			title: "Success"
			input: {
				string:  "yes"
				float:   0.0
				"null":  null
				integer: 1
				boolean: false
			}
			source: """
				.b1 = to_bool(.string)
				.b2 = to_bool(.float)
				.b3 = to_bool(.null)
				.b4 = to_bool(.integer)
				.b5 = to_bool(.boolean)
				"""
			output: {
				b1: true
				b2: false
				b3: false
				b4: true
				b5: false
			}
		},
		{
			title: "Error"
			input: {
				string: "definitely will not work"
			}
			source: ".bool = to_bool(.string)"
			output: {
				error: remap.errors.ArgumentError
			}
		},
	]
}
