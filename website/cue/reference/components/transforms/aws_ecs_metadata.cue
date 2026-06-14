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
				Do not enable this transform if you are running Vector as an Aggregator.  Metadata
				will be sourced from the Aggregator task's metadata endpoint and not the originating task's endpoint.
				""",
		]
		notices: []
	}

	configuration: generated.components.transforms.aws_ecs_metadata.configuration

	env_vars: {
		ECS_CONTAINER_METADATA_URI_V4: {
			description: "The Amazon ECS task metadata endpoint version 4 base URI."
			type: string: {}
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

	_namespace_note: """
		Field and tag names are shown using the default `namespace` value, `aws.ecs`.
		If `namespace` is changed, this prefix changes accordingly. If `namespace` is
		empty, fields and tags are emitted without a prefix.
		"""

	output: logs: log: {
		description: "Log event enriched with ECS metadata.\n\n\(_namespace_note)"
		fields: {
			"aws.ecs.availability-zone": {
				description: "The Availability Zone where the ECS task is running."
				required:    false
				type: string: examples: ["us-east-1a"]
			}
			"aws.ecs.cluster": {
				description: "The ECS cluster ARN or short name."
				required:    false
				type: string: examples: ["arn:aws:ecs:us-east-1:123456789012:cluster/example"]
			}
			"aws.ecs.container-arn": {
				description: "The selected ECS container ARN."
				required:    false
				type: string: examples: ["arn:aws:ecs:us-east-1:123456789012:container/example/abc/vector"]
			}
			"aws.ecs.container-created-at": {
				description: "The selected ECS container creation timestamp."
				required:    false
				type: string: examples: ["2026-06-05T01:00:02Z"]
			}
			"aws.ecs.container-desired-status": {
				description: "The desired status of the selected ECS container."
				required:    false
				type: string: examples: ["RUNNING"]
			}
			"aws.ecs.container-exit-code": {
				description: "The selected ECS container exit code."
				required:    false
				type: int: examples: [0]
			}
			"aws.ecs.container-finished-at": {
				description: "The selected ECS container finish timestamp."
				required:    false
				type: string: examples: ["2026-06-05T01:00:04Z"]
			}
			"aws.ecs.container-id": {
				description: "The selected ECS container Docker ID."
				required:    false
				type: string: examples: ["bfa2636268144d039771334145e490c5-1117626119"]
			}
			"aws.ecs.container-known-status": {
				description: "The known status of the selected ECS container."
				required:    false
				type: string: examples: ["RUNNING"]
			}
			"aws.ecs.container-name": {
				description: "The selected ECS container name."
				required:    false
				type: string: examples: ["vector"]
			}
			"aws.ecs.container-started-at": {
				description: "The selected ECS container start timestamp."
				required:    false
				type: string: examples: ["2026-06-05T01:00:03Z"]
			}
			"aws.ecs.container-type": {
				description: "The selected ECS container type."
				required:    false
				type: string: examples: ["NORMAL"]
			}
			"aws.ecs.desired-status": {
				description: "The desired status of the ECS task."
				required:    false
				type: string: examples: ["RUNNING"]
			}
			"aws.ecs.docker-name": {
				description: "The selected ECS container Docker name."
				required:    false
				type: string: examples: ["vector"]
			}
			"aws.ecs.execution-stopped-at": {
				description: "The ECS task execution stop timestamp."
				required:    false
				type: string: examples: ["2026-06-05T01:00:05Z"]
			}
			"aws.ecs.family": {
				description: "The ECS task definition family."
				required:    false
				type: string: examples: ["vector-task"]
			}
			"aws.ecs.fault-injection-enabled": {
				description: "Whether ECS fault injection is enabled for the task."
				required:    false
				type: bool: {}
			}
			"aws.ecs.image": {
				description: "The selected ECS container image."
				required:    false
				type: string: examples: ["public.ecr.aws/vector/vector:latest"]
			}
			"aws.ecs.image-id": {
				description: "The selected ECS container image ID."
				required:    false
				type: string: examples: ["sha256:vector"]
			}
			"aws.ecs.known-status": {
				description: "The known status of the ECS task."
				required:    false
				type: string: examples: ["RUNNING"]
			}
			"aws.ecs.launch-type": {
				description: "The ECS task launch type."
				required:    false
				type: string: examples: ["EC2", "FARGATE", "MANAGED_INSTANCES"]
			}
			"aws.ecs.log-driver": {
				description: "The selected ECS container log driver."
				required:    false
				type: string: examples: ["awslogs"]
			}
			"aws.ecs.pull-started-at": {
				description: "The ECS task image pull start timestamp."
				required:    false
				type: string: examples: ["2026-06-05T01:00:00Z"]
			}
			"aws.ecs.pull-stopped-at": {
				description: "The ECS task image pull stop timestamp."
				required:    false
				type: string: examples: ["2026-06-05T01:00:01Z"]
			}
			"aws.ecs.restart-count": {
				description: "The selected ECS container restart count."
				required:    false
				type: int: examples: [2]
			}
			"aws.ecs.revision": {
				description: "The ECS task definition revision."
				required:    false
				type: string: examples: ["7"]
			}
			"aws.ecs.service-name": {
				description: "The ECS service name."
				required:    false
				type: string: examples: ["vector-service"]
			}
			"aws.ecs.snapshotter": {
				description: "The selected ECS container snapshotter."
				required:    false
				type: string: examples: ["overlayfs"]
			}
			"aws.ecs.task-arn": {
				description: "The ECS task ARN."
				required:    false
				type: string: examples: ["arn:aws:ecs:us-east-1:123456789012:task/example/abc"]
			}
			"aws.ecs.vpc-id": {
				description: "The ECS task VPC ID."
				required:    false
				type: string: examples: ["vpc-1234567890abcdef0"]
			}
		}
	}

	output: metrics: metric: {
		description: "Metric event enriched with ECS metadata tags.\n\n\(_namespace_note)"
		tags: {
			"aws.ecs.availability-zone": {
				description: "The Availability Zone where the ECS task is running."
				required:    false
				examples: ["us-east-1a"]
			}
			"aws.ecs.cluster": {
				description: "The ECS cluster ARN or short name."
				required:    false
				examples: ["arn:aws:ecs:us-east-1:123456789012:cluster/example"]
			}
			"aws.ecs.container-arn": {
				description: "The selected ECS container ARN."
				required:    false
				examples: ["arn:aws:ecs:us-east-1:123456789012:container/example/abc/vector"]
			}
			"aws.ecs.container-created-at": {
				description: "The selected ECS container creation timestamp."
				required:    false
				examples: ["2026-06-05T01:00:02Z"]
			}
			"aws.ecs.container-desired-status": {
				description: "The desired status of the selected ECS container."
				required:    false
				examples: ["RUNNING"]
			}
			"aws.ecs.container-exit-code": {
				description: "The selected ECS container exit code."
				required:    false
				examples: ["0"]
			}
			"aws.ecs.container-finished-at": {
				description: "The selected ECS container finish timestamp."
				required:    false
				examples: ["2026-06-05T01:00:04Z"]
			}
			"aws.ecs.container-id": {
				description: "The selected ECS container Docker ID."
				required:    false
				examples: ["bfa2636268144d039771334145e490c5-1117626119"]
			}
			"aws.ecs.container-known-status": {
				description: "The known status of the selected ECS container."
				required:    false
				examples: ["RUNNING"]
			}
			"aws.ecs.container-name": {
				description: "The selected ECS container name."
				required:    false
				examples: ["vector"]
			}
			"aws.ecs.container-started-at": {
				description: "The selected ECS container start timestamp."
				required:    false
				examples: ["2026-06-05T01:00:03Z"]
			}
			"aws.ecs.container-type": {
				description: "The selected ECS container type."
				required:    false
				examples: ["NORMAL"]
			}
			"aws.ecs.desired-status": {
				description: "The desired status of the ECS task."
				required:    false
				examples: ["RUNNING"]
			}
			"aws.ecs.docker-name": {
				description: "The selected ECS container Docker name."
				required:    false
				examples: ["vector"]
			}
			"aws.ecs.execution-stopped-at": {
				description: "The ECS task execution stop timestamp."
				required:    false
				examples: ["2026-06-05T01:00:05Z"]
			}
			"aws.ecs.family": {
				description: "The ECS task definition family."
				required:    false
				examples: ["vector-task"]
			}
			"aws.ecs.fault-injection-enabled": {
				description: "Whether ECS fault injection is enabled for the task."
				required:    false
				examples: ["false"]
			}
			"aws.ecs.image": {
				description: "The selected ECS container image."
				required:    false
				examples: ["public.ecr.aws/vector/vector:latest"]
			}
			"aws.ecs.image-id": {
				description: "The selected ECS container image ID."
				required:    false
				examples: ["sha256:vector"]
			}
			"aws.ecs.known-status": {
				description: "The known status of the ECS task."
				required:    false
				examples: ["RUNNING"]
			}
			"aws.ecs.launch-type": {
				description: "The ECS task launch type."
				required:    false
				examples: ["EC2", "FARGATE", "MANAGED_INSTANCES"]
			}
			"aws.ecs.log-driver": {
				description: "The selected ECS container log driver."
				required:    false
				examples: ["awslogs"]
			}
			"aws.ecs.pull-started-at": {
				description: "The ECS task image pull start timestamp."
				required:    false
				examples: ["2026-06-05T01:00:00Z"]
			}
			"aws.ecs.pull-stopped-at": {
				description: "The ECS task image pull stop timestamp."
				required:    false
				examples: ["2026-06-05T01:00:01Z"]
			}
			"aws.ecs.restart-count": {
				description: "The selected ECS container restart count."
				required:    false
				examples: ["2"]
			}
			"aws.ecs.revision": {
				description: "The ECS task definition revision."
				required:    false
				examples: ["7"]
			}
			"aws.ecs.service-name": {
				description: "The ECS service name."
				required:    false
				examples: ["vector-service"]
			}
			"aws.ecs.snapshotter": {
				description: "The selected ECS container snapshotter."
				required:    false
				examples: ["overlayfs"]
			}
			"aws.ecs.task-arn": {
				description: "The ECS task ARN."
				required:    false
				examples: ["arn:aws:ecs:us-east-1:123456789012:task/example/abc"]
			}
			"aws.ecs.vpc-id": {
				description: "The ECS task VPC ID."
				required:    false
				examples: ["vpc-1234567890abcdef0"]
			}
		}
	}
}
