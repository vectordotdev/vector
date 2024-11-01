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
				string (or matches the default port for the scheme), it is
				populated from well-known ports for the following schemes:
				`http`, `https`, `ws`, `wss`, and `ftp`.
				"""
			required: false
			type: ["boolean"]
			default: false
		},
	]
	internal_failure_reasons: [
		"`value` is not a properly formatted URL.",
	]
	return: types: ["object"]

	examples: [
		{
			title: "Parse URL"
			source: #"""
				parse_url!("ftp://foo:bar@example.com:4343/foobar?hello=world#123")
				"""#
			return: {
				scheme:   "ftp"
				username: "foo"
				password: "bar"
				host:     "example.com"
				port:     4343
				path:     "/foobar"
				query: hello: "world"
				fragment: "123"
			}
		},
		{
			title: "Parse URL with default port"
			source: #"""
				parse_url!("https://example.com", default_known_ports: true)
				"""#
			return: {
				scheme:   "https"
				username: ""
				password: ""
				host:     "example.com"
				port:     443
				path:     "/"
				query: {}
				fragment: null
			}
		},
		{
			title: "Parse URL with internationalized domain name"
			source: #"""
				parse_url!("https://www.café.com")
				"""#
			return: {
				scheme:   "https"
				username: ""
				password: ""
				host:     "www.xn--caf-dma.com"
				port:     null
				path:     "/"
				query: {}
				fragment: null
			}
		},
		{
			title: "Parse URL with mixed case internationalized domain name"
			source: #"""
				parse_url!("https://www.CAFé.com")
				"""#
			return: {
				scheme:   "https"
				username: ""
				password: ""
				host:     "www.xn--caf-dma.com"
				port:     null
				path:     "/"
				query: {}
				fragment: null
			}
		},
	]
}
