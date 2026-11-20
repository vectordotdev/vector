package metadata

_schemaDefinitions: "core::option::Option<vector::kafka::AwsMskIamConfig>": object: options: {
	auth: {
		description: "Configuration of the authentication strategy for interacting with AWS services."
		required:    false
		type:        _schemaDefinitions["vector::aws::auth::AwsAuthentication"]
	}
	enabled: {
		description: "Enables MSK IAM authentication."
		required:    true
		type: bool: {}
	}
	endpoint: {
		description: "Custom endpoint for use with AWS-compatible services."
		required:    false
		type: string: examples: ["http://127.0.0.0:5000/path/to/service"]
	}
	region: {
		description: """
			The [AWS region][aws_region] of the target service.

			[aws_region]: https://docs.aws.amazon.com/general/latest/gr/rande.html#regional-endpoints
			"""
		required: false
		type: string: examples: [
			"us-east-1"
		]
	}
}
