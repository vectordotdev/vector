remap: functions: to_timestamp: {
	arguments: [
		{
			name:        "value"
			description: "The value that is to be converted to a timestamp. If a string, must be a valid representation of a `timestamp`, and no `default` exists, an `ArgumentError` will be raised."
			required:    true
			type: ["string", "integer", "timestamp"]
		},
	]
	return: ["timestamp"]
	category: "Coerce"
	description: #"""
		Returns a `timestamp` from the parameters. If parameter is `string`, the timestamp is parsed in these formats.
		If parameter is `integer`, the timestamp is taken as that number of seconds after January 1st, 1970.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				date: "2020-10-21T16:00:00Z"
			}
			source: ".date = to_timestamp(.date)"
			output: {
				date: "2020-10-21T16:00:00Z"
			}
		},
	]
}
