package metadata

base: api: configuration: api: {
	address: {
		description: """
			The network address to which the API should bind. If you're running
			Vector in a Docker container, make sure to bind to `0.0.0.0`. Otherwise
			the API will not be exposed outside the container.
			"""
		required: false
		type: string: {
			default: "127.0.0.1:8686"
			examples: ["0.0.0.0:8686", "127.0.0.1:1234"]
		}
	}
	enabled: {
		description: "Whether the GraphQL API is enabled for this Vector instance."
		required:    false
		type: bool: default: false
	}
	graphql: {
		description: """
			Whether the endpoint for receiving and processing GraphQL queries is
			enabled for the API. The endpoint is accessible via the `/graphql`
			endpoint of the address set using the `bind` parameter.
			"""
		required: false
		type: bool: default: true
	}
	playground: {
		description: """
			Whether the [GraphQL Playground]((urls.graphql_playground)) is enabled
			for the API. The Playground is accessible via the `/playground` endpoint
			of the address set using the `bind` parameter. Note that the `playground`
			endpoint will only be enabled if the `graphql` endpoint is also enabled.
			"""
		required: false
		type: bool: default: true
	}
}
