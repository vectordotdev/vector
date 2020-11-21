package metadata

// These sources produce JSON providing a structured representation of the
// Vector GraphQL API

api: {
	description:     !=""
	playground_url:  !=""
	schema_json_url: !=""
	configuration:   #Schema
}

api: {
	description:     """
		The [GraphQL](\(urls.graphql)) API exposed by Vector for configuration,
		monitoring, and topology visualization.
		"""
	playground_url:  "https://playground.vector.dev:8686/playground"
	schema_json_url: "https://github.com/timberio/vector/blob/master/lib/vector-api-client/graphql/schema.json"
	configuration: {
		enabled: {
			common: true
			type: bool: default: false
			required:    false
			description: "Whether the GraphQL API is enabled for this Vector instance."
		}
		address: {
			common:   true
			required: false
			type: string: {
				default: "127.0.0.1:8686"
				examples: ["0.0.0.0:8686", "localhost:1234"]
			}
			description: """
				The network address to which the API should bind. If you're running
				Vector in a Docker container, make sure to bind to `0.0.0.0`. Otherwise
				the API will not be exposed outside the container.
				"""
		}
		playground: {
			common:   false
			required: false
			type: bool: default: true
			description: """
				Whether the [GraphQL Playground](\(urls.graphql_playground)) is enabled
				for the API. The Playground is accessible via the `/playground` endpoint
				of the address set using the `bind` parameter.
				"""
		}
	}
}
