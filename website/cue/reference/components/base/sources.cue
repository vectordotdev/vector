package metadata

base: components: sources: configuration: proxy: {
	description: """
		Proxy configuration.

		Configure to proxy traffic through an HTTP(S) proxy when making external requests.

		Similar to common proxy configuration convention, you can set different proxies
		to use based on the type of traffic being proxied, as well as set specific hosts that
		should not be proxied.
		"""
	required: false
	type: object: options: {
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
}
