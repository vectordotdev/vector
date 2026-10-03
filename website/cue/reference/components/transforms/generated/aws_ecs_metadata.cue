package metadata

generated: components: transforms: aws_ecs_metadata: configuration: {
	container_name: {
		description: """
			The name of the container to enrich events with.

			If unset, the transform uses the current container's name from the ECS task metadata endpoint.
			"""
		required: false
		type: string: examples: ["vector", "app"]
	}
	endpoint: {
		description: "Overrides the ECS task metadata endpoint."
		required:    false
		type: string: examples: ["http://169.254.170.2/v4/example"]
	}
	fields: {
		description: "A list of metadata fields to include in each transformed event."
		required:    false
		type: array: {
			default: ["cluster", "task-arn", "family", "revision", "service-name", "launch-type", "availability-zone", "container-name", "container-id", "container-arn", "image", "image-id"]
			items: type: string: examples: ["task-arn", "container-name"]
		}
	}
	initial_retry_attempts: {
		description: "The number of times to attempt fetching metadata at startup before Vector begins processing events."
		required:    false
		type: uint: default: 3
	}
	initial_retry_backoff_secs: {
		description: "The delay between initial metadata refresh attempts, in seconds."
		required:    false
		type: uint: {
			default: 1
			unit:    "seconds"
		}
	}
	namespace: {
		description: "Sets a prefix for all event fields added by the transform."
		required:    false
		type: string: {
			default: ".aws.ecs"
			examples: ["", "ecs", "aws.ecs"]
		}
	}
	proxy: {
		description: """
			Proxy configuration.

			Configure to proxy traffic through an HTTP(S) proxy when making external requests.

			Similar to common proxy configuration convention, you can set different proxies
			to use based on the type of traffic being proxied. You can also set specific hosts that
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
	refresh_interval_secs: {
		description: "Interval between metadata refresh requests, in seconds."
		required:    false
		type: uint: {
			default: 10
			unit:    "seconds"
		}
	}
	refresh_timeout_secs: {
		description: "The timeout for querying the ECS metadata endpoint, in seconds."
		required:    false
		type: uint: {
			default: 1
			unit:    "seconds"
		}
	}
	required: {
		description: "Requires the transform to successfully query the ECS metadata endpoint before processing events."
		required:    false
		type: bool: default: true
	}
}
