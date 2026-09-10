package metadata

_schemaDefinitions: "vector_core::config::proxy::ProxyConfig": object: options: {
	enabled: {
		description: "Enables proxying support."
		required:    false
		type: bool: default: true
	}
	http: {
		description: """
			Proxy endpoint to use when proxying HTTP traffic.

			Must be a valid URI string.
			"""
		required: false
		type: string: examples: ["http://foo.bar:3128"]
	}
	https: {
		description: """
			Proxy endpoint to use when proxying HTTPS traffic.

			Must be a valid URI string.
			"""
		required: false
		type: string: examples: ["http://foo.bar:3128"]
	}
	no_proxy: {
		description: """
			A list of hosts to avoid proxying.

			Multiple patterns are allowed:

			| Pattern             | Example match                                                               |
			| ------------------- | --------------------------------------------------------------------------- |
			| Domain names        | `example.com` matches requests to `example.com`                     |
			| Wildcard domains    | `.example.com` matches requests to `example.com` and its subdomains |
			| IP addresses        | `127.0.0.1` matches requests to `127.0.0.1`                         |
			| [CIDR][cidr] blocks | `192.168.0.0/16` matches requests to any IP addresses in this range     |
			| Splat               | `*` matches all hosts                                                   |

			[cidr]: https://en.wikipedia.org/wiki/Classless_Inter-Domain_Routing
			"""
		required: false
		type: array: {
			default: []
			items: type: string: examples: ["localhost", ".foo.bar", "*"]
		}
	}
}
