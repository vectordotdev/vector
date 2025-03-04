package metadata

// These sources produce JSON providing a structured representation of the
// Vector GraphQL API

api: {
	description:     !=""
	schema_json_url: !=""
	configuration:   #Schema
	endpoints:       #Endpoints
}

api: {
	description:     """
		The Vector [GraphQL](\(urls.graphql)) API allows you to interact with a
		running Vector instance, enabling introspection and management of
		Vector in real-time.
		"""
	schema_json_url: "https://github.com/vectordotdev/vector/blob/master/lib/vector-api-client/graphql/schema.json"
	configuration:   base.api.configuration.api

	endpoints: {
		"/graphql": {
			POST: {
				description: """
					Main endpoint for receiving and processing
					GraphQL queries.
					"""
				responses: {
					"200": {
						description: """
							The query has been processed. GraphQL returns 200
							regardless if the query was successful or not. This
							is due to the fact that queries can partially fail.
							Please check for the `errors` key to determine if
							there were any errors in your query.
							"""
					}
				}
			}
		}
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
		"/playground": {
			GET: {
				description: """
					A bundled GraphQL playground that enables you
					to explore the available queries and manually
					run queries.
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
