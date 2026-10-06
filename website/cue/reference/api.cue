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
					HTTP healthcheck endpoint served on the same port as the
					gRPC API, preserved for compatibility with Vector 0.54.0
					and earlier so existing HTTP probes (for example AWS ALB
					and Kubernetes HTTP probes) keep working unchanged.
					The response body is `{"ok": true}` while Vector is
					serving and `{"ok": false}` once Vector begins draining.
					"""
				responses: {
					"200": {
						description: "Vector is initialized and running."
					}
					"503": {
						description: "Vector is draining or shutting down and should be removed from the load balancer."
					}
				}
			}
			HEAD: {
				description: """
					Same semantics as `GET /health` but returns no body.
					Intended for load balancer probes that prefer `HEAD`.
					"""
				responses: {
					"200": {
						description: "Vector is initialized and running."
					}
					"503": {
						description: "Vector is draining or shutting down and should be removed from the load balancer."
					}
				}
			}
		}
	}
}
