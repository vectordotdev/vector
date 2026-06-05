package metadata

components: transforms: aws_ecs_metadata: {
	title: "AWS ECS Metadata"

	description: """
		Enriches events with AWS ECS task metadata.
		"""

	classes: {
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		enrich: {
			from: service: {
				name:     "AWS ECS task metadata"
				url:      urls.aws_ecs_task_metadata
				versions: "v4"
			}
		}
	}

	support: {
		requirements: [
			"""
				The transform must run inside an Amazon ECS task where the task metadata endpoint
				version 4 is available through `ECS_CONTAINER_METADATA_URI_V4`, unless `endpoint`
				is configured explicitly.
				""",
		]
		warnings: [
			"""
				Do not enable this transform if you are running Vector as an Aggregator, metadata
				will be sourced from the Aggregator task's metadata endpoint and not the client's.
				""",
		]
		notices: []
	}

	configuration: generated.components.transforms.aws_ecs_metadata.configuration

	env_vars: {
		ECS_CONTAINER_METADATA_URI_V4: {
			description: "The Amazon ECS task metadata endpoint version 4 base URI."
		}
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
		description: "Log event enriched with ECS metadata"
		fields: {
			"aws.ecs.cluster": {
				description: "The ECS cluster ARN or short name."
				required:    false
				type: string: examples: ["arn:aws:ecs:us-east-1:123456789012:cluster/example"]
			}
			"aws.ecs.task-arn": {
				description: "The ECS task ARN."
				required:    false
				type: string: examples: ["arn:aws:ecs:us-east-1:123456789012:task/example/abc"]
			}
			"aws.ecs.container-name": {
				description: "The selected ECS container name."
				required:    false
				type: string: examples: ["vector"]
			}
			"aws.ecs.container-id": {
				description: "The selected ECS container Docker ID."
				required:    false
				type: string: examples: ["bfa2636268144d039771334145e490c5-1117626119"]
			}
		}
	}
	output: metrics: metric: {
		description: "Metric event enriched with ECS metadata tags"
		tags: {
			"aws.ecs.cluster": {
				description: "The ECS cluster ARN or short name."
				required:    false
				examples: ["arn:aws:ecs:us-east-1:123456789012:cluster/example"]
			}
			"aws.ecs.task-arn": {
				description: "The ECS task ARN."
				required:    false
				examples: ["arn:aws:ecs:us-east-1:123456789012:task/example/abc"]
			}
			"aws.ecs.container-name": {
				description: "The selected ECS container name."
				required:    false
				examples: ["vector"]
			}
			"aws.ecs.container-id": {
				description: "The selected ECS container Docker ID."
				required:    false
				examples: ["bfa2636268144d039771334145e490c5-1117626119"]
			}
		}
	}
}
