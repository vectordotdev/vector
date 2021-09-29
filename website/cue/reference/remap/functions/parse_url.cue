package metadata

remap: functions: parse_url: {
	category:    "Parse"
	description: """
		Parses the `value` in [URL](\(urls.url)) format.

		The port number will be defaulted if unspecified in the URL and the scheme is one of: `http`, `https`, `ws`, `wss`, and `ftp`, schemes.
		"""

	arguments: [
		{
			name:        "value"
			description: "The text of the URL."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` isn't a properly formatted URL",
	]
	return: types: ["object"]

	examples: [
		{
			title: "Parse URL"
			source: #"""
				parse_url!("ftp://foo:bar@vector.dev:4343/foobar?hello=world#123")
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
