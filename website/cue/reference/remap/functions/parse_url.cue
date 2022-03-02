package metadata

remap: functions: parse_url: {
	category:    "Parse"
	description: """
		Parses the `value` in [URL](\(urls.url)) format.
		"""

	arguments: [
		{
			name:        "value"
			description: "The text of the URL."
			required:    true
			type: ["string"]
		},
		{
			name: "default_known_ports"
			description: """
				If true and the port number is not specified in the input URL
				string (or matches the default port for the scheme), it will be
				populated from well-known ports for the following schemes:
				`http`, `https`, `ws`, `wss`, and `ftp`.
				"""
			required: false
			type: ["boolean"]
			default: false
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
		{
			title: "Parse URL with default port"
			source: #"""
				parse_url!("https://vector.dev", default_known_ports: true)
				"""#
			return: {
				scheme:   "https"
				username: ""
				password: ""
				host:     "vector.dev"
				port:     443
				path:     "/"
				query: {}
				fragment: null
			}
		},
	]
}
