package metadata

remap: functions: seahash: {
	category:    "Cryptography"
	description: """
		Calculates a [Seahash](\(urls.seahash)) hash of the `value`.
		Note: function converts unsigned int-64 seahash to signed int-64 to met vrl standards, so integer overflow happens when calculated hash is higher than signed int-64 max value. Depending on the use-case this might or mightn't be an issue.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to calculate the hash for."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Calculate seahash"
			source: #"""
				seahash("foobar")
				"""#
			return: "5348458858952426560"
		},
		{
			title: "Calculate negative seahash"
			source: #"""
				seahash("bar")
				"""#
			return: "-2796170501982571315"
		},
	]
}
