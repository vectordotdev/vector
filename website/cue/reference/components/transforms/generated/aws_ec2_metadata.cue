package metadata

generated: components: transforms: aws_ec2_metadata: configuration: {
	endpoint: {
		description: "Overrides the default EC2 metadata endpoint."
		required:    false
		type: string: default: "http://169.254.169.254"
	}
	fields: {
		description: "A list of metadata fields to include in each transformed event."
		required:    false
		type: array: {
			default: ["ami-id", "availability-zone", "instance-id", "instance-type", "local-hostname", "local-ipv4", "public-hostname", "public-ipv4", "region", "subnet-id", "vpc-id", "role-name"]
			items: type: string: examples: ["instance-id", "local-hostname"]
		}
	}
	namespace: {
		description: "Sets a prefix for all event fields added by the transform."
		required:    false
		type: string: examples: ["", "ec2", "aws.ec2"]
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
		type:     _schemaDefinitions["vector_core::config::proxy::ProxyConfig"]
	}
	refresh_interval_secs: {
		description: "The interval between querying for updated metadata, in seconds."
		required:    false
		type: uint: {
			default: 10
			unit:    "seconds"
		}
	}
	refresh_timeout_secs: {
		description: "The timeout for querying the EC2 metadata endpoint, in seconds."
		required:    false
		type: uint: {
			default: 1
			unit:    "seconds"
		}
	}
	required: {
		description: "Requires the transform to be able to successfully query the EC2 metadata before starting to process the data."
		required:    false
		type: bool: default: true
	}
	tags: {
		description: "A list of instance tags to include in each transformed event."
		required:    false
		type: array: {
			default: []
			items: type: string: examples: ["Name", "Project"]
		}
	}
}
