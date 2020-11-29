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
	return: ["map"]
	category: "parse"
	description: #"""
		Parses a url into it's constituent components.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				url: #"ftp://foo:bar@vector.dev:4343/foobar?hello=world#123"#
			}
			source: #"""
				.parsed = parse_url(.url)
				"""#
			output: {
				url: #"ftp://foo:bar@vector.dev:4343/foobar?hello=world#123"#
				parsed: {
					"scheme":   "ftp"
					"username": "foo"
					"password": "bar"
					"host":     "vector.dev"
					"port":     4343
					"path":     "/foobar"
					"query": {"hello": "world"}
					"fragment": "123"
				}
			}
		},
		{
			title: "Error"
			input: {
				url: "I am not a url"
			}
			source: #"""
				.parsed = parse_url(.url)
				"""#
			output: {
				error: remap.errors.ParseError
			}
		},
	]
}
