package metadata

components: transforms: aws_ec2_metadata: {
	title: "AWS EC2 Metadata"

	description: """
		Enriches events with AWS EC2 environment metadata.
		"""

	classes: {
		commonly_used: false
		development:   "stable"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		enrich: {
			from: service: {
				name:     "AWS EC2 instance metadata"
				url:      urls.aws_ec2_instance_metadata
				versions: ">= 2"
			}
		}
	}

	support: {
		requirements: [
			"""
				Running this transform within Docker on EC2 requires 2 network hops. Users must raise this limit:

				```bash
				aws ec2 modify-instance-metadata-options --instance-id <ID> --http-endpoint enabled --http-put-response-hop-limit 2
				```
				""",
		]
		notices: []
		warnings: [
			"""
				Do not enable this transform if you are running Vector as an Aggregator, tags will be sourced from the Aggregator node's metadata server and not the client's.
				""",
		]
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
			type: array: {
				default: ["instance-id", "local-hostname", "local-ipv4", "public-hostname", "public-ipv4", "ami-id", "availability-zone", "vpc-id", "subnet-id", "region"]
				items: type: string: {
					examples: ["instance-id", "local-hostname"]
				}
			}
		}
		namespace: {
			common:      true
			description: "Prepend a namespace to each field's key."
			required:    false
			type: string: {
				default: ""
				examples: ["", "ec2", "aws.ec2"]
			}
		}
		proxy: configuration._proxy
		refresh_interval_secs: {
			common:      true
			description: "The interval in seconds at which the EC2 Metadata api will be called."
			required:    false
			type: uint: {
				default: 10
				unit:    null
			}
		}
	}

	env_vars: {
		http_proxy:  env_vars._http_proxy
		HTTP_PROXY:  env_vars._http_proxy
		https_proxy: env_vars._https_proxy
		HTTPS_PROXY: env_vars._https_proxy
		no_proxy:    env_vars._no_proxy
		NO_PROXY:    env_vars._no_proxy
	}

	input: {
		logs: true
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
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
			"instance-type": {
				description: "The `instance-type` of the current EC2 instance."
				required:    true
				type: string: {
					examples: ["m4.large"]
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
		metadata_refresh_failed_total:     components.sources.internal_metrics.output.metrics.metadata_refresh_failed_total
		metadata_refresh_successful_total: components.sources.internal_metrics.output.metrics.metadata_refresh_successful_total
	}
}
