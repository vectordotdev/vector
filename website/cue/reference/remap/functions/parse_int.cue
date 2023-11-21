package metadata

remap: functions: parse_int: {
	category: "Parse"
	description: #"""
		Parses the string `value` representing a number in an optional base/radix to an integer.
		"""#

	arguments: [
		{
			name:        "value"
			description: "The string to parse."
			required:    true
			type: ["string"]
		},
		{
			name: "base"
			description: """
				The base the number is in. Must be between 2 and 36 (inclusive).

				If unspecified, the string prefix is used to
				determine the base: "0b", 8 for "0" or "0o", 16 for "0x",
				and 10 otherwise.
				"""
			required: false
			type: ["integer"]
		},
	]
	internal_failure_reasons: [
		"The base is not between 2 and 36.",
		"The number cannot be parsed in the base.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Parse decimal"
			source: #"""
				parse_int!("-42")
				"""#
			return: -42
		},
		{
			title: "Parse binary"
			source: #"""
				parse_int!("0b1001")
				"""#
			return: 9
		},
		{
			title: "Parse octal"
			source: #"""
				parse_int!("0o42")
				"""#
			return: 34
		},
		{
			title: "Parse hexadecimal"
			source: #"""
				parse_int!("0x2a")
				"""#
			return: 42
		},
		{
			title: "Parse explicit base"
			source: #"""
				parse_int!("2a", 17)
				"""#
			return: 44
		},
	]
}
