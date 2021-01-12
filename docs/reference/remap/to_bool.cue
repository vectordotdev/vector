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
			title: "Cast a value to a boolean"
			input: log: {
				string:  "yes"
				float:   0.0
				"null":  null
				integer: 1
				boolean: false
			}
			source: """
				.string = to_bool(.string)
				.float = to_bool(.float)
				.null = to_bool(.null)
				.integer = to_bool(.integer)
				.boolean = to_bool(.boolean)
				"""
			output: log: {
				string:  true
				float:   false
				null:    false
				integer: true
				boolean: false
			}
		},
	]
}
