package metadata

remap: functions: parse_url: {
	arguments: [
		{
			name:        "value"
			description: "The text of the url."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a properly formatted URL",
	]
	return: ["map"]
	category: "Parse"
	description: #"""
		Parses the provided `value` in URL format.
		"""#
	examples: [
		{
			title: "Parse a URL (success)"
			input: log: url: #"ftp://foo:bar@vector.dev:4343/foobar?hello=world#123"#
			source: #"""
				.url = parse_url(del(.url))
				"""#
			output: log: url: {
				scheme:   "ftp"
				username: "foo"
				password: "bar"
				host:     "vector.dev"
				port:     4343
				path:     "/foobar"
				query: hello: "world"
				fragment: "123"
			}
		},
		{
			title: "Parse a URL (error)"
			input: log: url: "I am not a url"
			source: #"""
				.url = parse_url(del(.url))
				"""#
			raise: "Failed to parse"
		},
	]
}
