package metadata

base: components: sinks: openobserve: configuration: {
    type: "http"
    inputs: ["source_or_transform_id"]
    uri: {
        description: "The OpenObserve endpoint to send data to."
        required: true
        type: string: examples: ["http://localhost:5080/api/default/default/_json"]
    }
    method: {
        description: "The HTTP method to use for the request."
        required: true
        type: string: default: "post"
    }
    auth: {
        description: "Authentication for OpenObserve."
        required: true
        type: object: options: {
            strategy: {
                description: "The authentication strategy."
                required: true
                type: string: default: "basic"
            }
            user: {
                description: "The username for basic authentication."
                required: true
                type: string: examples: ["test@example.com"]
            }
            password: {
                description: "The password for basic authentication."
                required: true
                type: string: examples: ["your_ingestion_password"]
            }
        }
    }
    compression: {
        description: "The compression algorithm to use."
        required: true
        type: string: default: "gzip"
    }
    encoding: {
        codec: {
            description: "The encoding format to use for the request body."
            required: true
            type: string: default: "json"
        }
        timestamp_format: {
            description: "The format for encoding timestamps."
            required: true
            type: string: default: "rfc3339"
        }
    }
    healthcheck: {
        enabled: {
            description: "Enables or disables the health check."
            required: true
            type: bool: default: false
        }
    }
}
