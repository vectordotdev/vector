remap: functions: to_bool: {
	arguments: [
		{
			name:        "value"
			description: "The value to convert to a Boolean."
			required:    true
			type: ["boolean", "integer", "float", "null", "string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a supported boolean representation",
	]
	return: ["float"]
	category: "Coerce"
	description: #"""
		Coerces the provided `value` into a `boolean`.

		The conversion rules vary by type:

		| Type      | `true` values | `false` values |
		|:----------|:--------------|:---------------|
		| `string`  | `"true"`, `"t"`, `"yes"`, `"y"` | `"false"`, `"f"`, `"no"`, `"n"`, `"0"` |
		| `float`   | == `0.0` | != `0.0` |
		| `int`     | == `0` | != `0` |
		| `null`    | | `null` |
		| `boolean` | `true` | `false` |
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
