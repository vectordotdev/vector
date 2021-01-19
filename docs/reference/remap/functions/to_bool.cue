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
			title: "Coerce to a boolean (string)"
			source: """
				to_bool("yes")
				"""
			return: true
		},
		{
			title: "Coerce to a boolean (float)"
			source: """
				to_bool(0.0)
				"""
			return: false
		},
		{
			title: "Coerce to a boolean (int)"
			source: """
				to_bool(0)
				"""
			return: false
		},
		{
			title: "Coerce to a boolean (null)"
			source: """
				to_bool(null)
				"""
			return: false
		},
		{
			title: "Coerce to a boolean (boolean)"
			source: """
				to_bool(true)
				"""
			return: true
		},
	]
}
