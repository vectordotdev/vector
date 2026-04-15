package metadata

// These sources produce JSON providing a structured representation of the
// Vector gRPC API

api: {
	description:     !=""
	schema_json_url: !=""
	configuration:   #Schema
	endpoints:       #Endpoints
}

api: {
	description: """
		The Vector gRPC API allows you to interact with a
		running Vector instance, enabling introspection and management of
		Vector in real-time. The service definition is available in
		`proto/vector/observability.proto`.
		"""
	schema_json_url: "https://github.com/vectordotdev/vector/blob/master/proto/vector/observability.proto"
	configuration:   generated.configuration.configuration.api.type.object.options

	endpoints: {
		"/health": {
			GET: {
				description: """
					Healthcheck endpoint. Useful to verify that
					Vector is up and running.
					"""
				responses: {
					"200": {
						description: "Vector is initialized and running."
					}
				}
			}
		}
	}
}
