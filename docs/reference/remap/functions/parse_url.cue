package metadata

remap: functions: parse_url: {
	category:    "Parse"
	description: """
		Parses the `value` in [URL](\(urls.url)) format.
		"""

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
	return: types: ["map"]

	examples: [
		{
			title: "Parse URL"
			source: #"""
				parse_url("ftp://foo:bar@vector.dev:4343/foobar?hello=world#123")
				"""#
			return: {
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
	]
}
