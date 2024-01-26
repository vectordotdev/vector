package metadata

base: components: sources: aws_ecs_metrics: configuration: {
	endpoint: {
		description: """
			Base URI of the task metadata endpoint.

			If empty, the URI is automatically discovered based on the latest version detected.

			By default:
			- The version 4 endpoint base URI is stored in the environment variable `ECS_CONTAINER_METADATA_URI_V4`.
			- The version 3 endpoint base URI is stored in the environment variable `ECS_CONTAINER_METADATA_URI`.
			- The version 2 endpoint base URI is `169.254.170.2/v2/`.
			"""
		required: false
		type: string: default: "http://169.254.170.2/v2"
	}
	namespace: {
		description: """
			The namespace of the metric.

			Disabled if empty.
			"""
		required: false
		type: string: default: "awsecs"
	}
	scrape_interval_secs: {
		description: "The interval between scrapes, in seconds."
		required:    false
		type: uint: {
			default: 15
			unit:    "seconds"
		}
	}
	version: {
		description: """
			The version of the task metadata endpoint to use.

			If empty, the version is automatically discovered based on environment variables.

			By default:
			- Version 4 is used if the environment variable `ECS_CONTAINER_METADATA_URI_V4` is defined.
			- Version 3 is used if the environment variable `ECS_CONTAINER_METADATA_URI_V4` is not defined, but the
			  environment variable `ECS_CONTAINER_METADATA_URI` _is_ defined.
			- Version 2 is used if neither of the environment variables `ECS_CONTAINER_METADATA_URI_V4` or
			  `ECS_CONTAINER_METADATA_URI` are defined.
			"""
		required: false
		type: string: {
			default: "v2"
			enum: {
				v2: """
					Version 2.

					More information about version 2 of the task metadata endpoint can be found [here][endpoint_v2].

					[endpoint_v2]: https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task-metadata-endpoint-v2.html
					"""
				v3: """
					Version 3.

					More information about version 3 of the task metadata endpoint can be found [here][endpoint_v3].

					[endpoint_v3]: https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task-metadata-endpoint-v3.html
					"""
				v4: """
					Version 4.

					More information about version 4 of the task metadata endpoint can be found [here][endpoint_v4].

					[endpoint_v4]: https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task-metadata-endpoint-v4.html
					"""
			}
		}
	}
}
