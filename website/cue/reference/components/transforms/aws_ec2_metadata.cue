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
			"""
				Accessing instance tags must be explicitly enabled for each instance. This can be done in the AWS Console, or with the following CLI command:

				```bash
				aws ec2 modify-instance-metadata-options --instance-id <ID> --instance-metadata-tags enabled
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

	configuration: base.components.transforms.aws_ec2_metadata.configuration

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
		traces: false
	}

	output: logs: log: {
		description: "Log event enriched with EC2 metadata"
		fields: {
			"account-id": {
				description: "The `account-id` that launched the EC2 instance."
				required:    false
				common:      true
				type: string: {
					default: null
					examples: ["123456789"]
				}
			}
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
			"tags": {
				description: "The instance's tags"
				required:    false
				type: object: {
					examples: [
						{
							"Name":          "InstanceName"
							"ApplicationId": "12345678"
						},
					]
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
