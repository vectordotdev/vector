package metadata

components: transforms: aws_ec2_metadata: {
	title: "AWS EC2 Metadata"

	classes: {
		commonly_used: false
		development:   "stable"
		egress_method: "stream"
	}

	features: {
		enrich: {
			from: {
				name:     "AWS EC2 instance metadata"
				url:      urls.aws_ec2_instance_metadata
				versions: ">= 2"
			}
		}
	}

	support: {
		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: [
			"""
				Running this transform within Docker on EC2 requires 2 network hops. Users must raise this limit:

				```bash
				aws ec2 modify-instance-metadata-options --instance-id <ID> --http-endpoint enabled --http-put-response-hop-limit 2
				```
				""",
		]
		warnings: []
		notices: []
	}

	configuration: {
		endpoint: {
			common:      false
			description: "Override the default EC2 Metadata endpoint."
			required:    false
			type: string: {
				default: "http://169.254.169.254"
			}
		}
		fields: {
			common:      true
			description: "A list of fields to include in each event."
			required:    false
			warnings: []
			type: array: {
				default: ["instance-id", "local-hostname", "local-ipv4", "public-hostname", "public-ipv4", "ami-id", "availability-zone", "vpc-id", "subnet-id", "region"]
				items: type: string: examples: ["instance-id", "local-hostname"]
			}
		}
		namespace: {
			common:      true
			description: "Prepend a namespace to each field's key."
			required:    false
			warnings: []
			type: string: {
				default: ""
				examples: ["", "ec2", "aws.ec2"]
			}
		}
		refresh_interval_secs: {
			common:      true
			description: "The interval in seconds at which the EC2 Metadata api will be called."
			required:    false
			warnings: []
			type: uint: {
				default: 10
				unit:    null
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	output: logs: log: {
		description: "Log event enriched with EC2 metadata"
		fields: {
			"ami-id": {
				description: "The `ami-id` that the current EC2 instance is using."
				required:    true
				type: string: {
					examples: ["ami-00068cd7555f543d5"]
				}
			}
			"availability-zone": {
				description: "The `availability-zone` that the current EC2 instance is running in."
				required:    true
				type: string: {
					examples: ["54.234.246.107"]
				}
			}
			"instance-id": {
				description: "The `instance-id` of the current EC2 instance."
				required:    true
				type: string: {
					examples: ["i-096fba6d03d36d262"]
				}
			}
			"local-hostname": {
				description: "The `local-hostname` of the current EC2 instance."
				required:    true
				type: string: {
					examples: ["ip-172-31-93-227.ec2.internal"]
				}
			}
			"local-ipv4": {
				description: "The `local-ipv4` of the current EC2 instance."
				required:    true
				type: string: {
					examples: ["172.31.93.227"]
				}
			}
			"public-hostname": {
				description: "The `public-hostname` of the current EC2 instance."
				required:    true
				type: string: {
					examples: ["ec2-54-234-246-107.compute-1.amazonaws.com"]
				}
			}
			"public-ipv4": {
				description: "The `public-ipv4` of the current EC2 instance."
				required:    true
				type: string: {
					examples: ["54.234.246.107"]
				}
			}
			"region": {
				description: "The `region` that the current EC2 instance is running in."
				required:    true
				type: string: {
					examples: ["us-east-1"]
				}
			}
			"role-name": {
				description: "The `role-name` that the current EC2 instance is using."
				required:    true
				type: string: {
					examples: ["some_iam_role"]
				}
			}
			"subnet-id": {
				description: "The `subnet-id` of the current EC2 instance's default network interface."
				required:    true
				type: string: {
					examples: ["subnet-9d6713b9"]
				}
			}
			"vpc-id": {
				description: "The `vpc-id` of the current EC2 instance's default network interface."
				required:    true
				type: string: {
					examples: ["vpc-a51da4dc"]
				}
			}
		}
	}

	telemetry: metrics: {
		vector_metadata_refresh_successful_total: _vector_metadata_refresh_successful_total
		vector_metadata_refresh_failed_total:     _vector_metadata_refresh_failed_total
	}
}
