package metadata

base: components: sources: opcua: configuration: {
	url: {
		description: """
			The OPC URL to connect to.

			The URL takes the form of `opc.tcp://server:port`.
			"""
		required: true
		type: string: examples: ["opc.tcp://localhost:4840"]
	}
	application_uri: {
		description: """
			The application URI to use when connecting to the server.

			This is used to identify the client to the server.
			"""
		required: true
		type: string: examples: ["urn:example:client"]
	}
	product_uri: {
		description: """
			The product URI to use when connecting to the server.

			This is used to identify the client to the server.
			"""
		required: true
		type: string: examples: ["urn:example:client"]
	}
	trust_server_certs: {
		description: """
			Whether to trust the server's certificate.

			If this is set to `true`, the client will trust the server's certificate
			without verifying it. This is insecure, but may be useful for testing.
			"""
		required: false
		type: boolean: examples: [true]
		default: false
	}
	create_sample_keypair: {
		description: """
			Whether to create a sample keypair for the client.

			If this is set to `true`, the client will create a sample keypair
			and use it to connect to the server. This is insecure, but may be useful
			for testing.
			"""
		required: false
		type: boolean: examples: [true]
		default: false
	}
	node_ids: {
		description: """
			A list of node IDs and name to subscribe to.
			"""
		required: true
		type: array: items: {
			type: object: properties: {
				node_id: {
					description: """
						The node ID to subscribe to.
						"""
					required: true
					type: string: examples: ["ns=0;i=2258"]
				}
				name: {
					description: """
						The name of the node.
						"""
					required: true
					type: string: examples: ["Temperature"]
				}
			}
		}
	}
}
