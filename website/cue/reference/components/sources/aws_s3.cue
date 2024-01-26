package metadata

components: sources: aws_s3: components._aws & {
	title: "AWS S3"

	features: {
		auto_generated:   true
		acknowledgements: true
		multiline: enabled: true
		collect: {
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
				enabled_by_scheme:      true
			}
			checkpoint: enabled: false
			proxy: enabled:      true
			from: service:       services.aws_s3
		}
	}

	classes: {
		commonly_used: true
		deployment_roles: ["aggregator"]
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "stream"
		stateful:      false
	}

	support: {
		requirements: [
			"""
				The AWS S3 source requires a SQS queue configured to receive S3
				bucket notifications for the desired S3 buckets.
				""",
		]
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: base.components.sources.aws_s3.configuration & {
		_aws_include: false
	}

	output: logs: object: {
		description: "A line from an S3 object."
		fields: {
			message: {
				description: "A line from the S3 object."
				required:    true
				type: string: {
					examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
				}
			}
			timestamp: fields._current_timestamp & {
				description: "The Last-Modified time of the object. Defaults the current timestamp if this information is missing."
			}
			source_type: {
				description: "The name of the source type."
				required:    true
				type: string: {
					examples: ["aws_s3"]
				}
			}
			bucket: {
				description: "The bucket of the object the line came from."
				required:    true
				type: string: {
					examples: ["my-bucket"]
				}
			}
			object: {
				description: "The object the line came from."
				required:    true
				type: string: {
					examples: ["AWSLogs/111111111111/vpcflowlogs/us-east-1/2020/10/26/111111111111_vpcflowlogs_us-east-1_fl-0c5605d9f1baf680d_20201026T1950Z_b1ea4a7a.log.gz"]
				}
			}
			region: {
				description: "The AWS region bucket is in."
				required:    true
				type: string: {
					examples: ["us-east-1"]
				}
			}
		}
	}

	how_it_works: {
		events: {
			title: "Handling events from the `aws_s3` source"
			body:  """
				This source behaves very similarly to the `file` source in that
				it will output one event per line (unless the `multiline`
				configuration option is used).

				You will commonly want to use [transforms](\(urls.vector_transforms)) to
				parse the data. For example, to parse VPC flow logs sent to S3 you can
				chain the `remap` transform:

				```toml
				[transforms.flow_logs]
				type = "remap" # required
				inputs = ["s3"]
				drop_on_error = false
				source = '''
				. = parse_aws_vpc_flow_log!(string!(.message))
				'''
				```

				To parse AWS load balancer logs, the `remap` transform can be used:

				```toml
				[transforms.elasticloadbalancing_fields_parsed]
				type = "remap" # required
				inputs = ["s3"]
				drop_on_error = false
				source = '''
				. = parse_aws_alb_log!(string!(.message))
				.request_url_parts = parse_url!(.request_url)
				'''
				```
				"""
		}
	}

	permissions: iam: [
		{
			platform:      "aws"
			_service:      "s3"
			_docs_tag:     "AmazonS3"
			_url_fragment: "API"

			policies: [
				{
					_action: "GetObject"
				},
			]
		},
		{
			platform:  "aws"
			_service:  "sqs"
			_docs_tag: "AWSSimpleQueueService"

			policies: [
				{
					_action: "ReceiveMessage"
				},
				{
					_action:       "DeleteMessage"
					required_when: "[`delete_message`](#sqs.delete_message) is set to `true`"
				},
			]
		},
	]

	telemetry: metrics: {
		sqs_message_delete_succeeded_total:     components.sources.internal_metrics.output.metrics.sqs_message_delete_succeeded_total
		sqs_message_processing_succeeded_total: components.sources.internal_metrics.output.metrics.sqs_message_processing_succeeded_total
		sqs_message_receive_succeeded_total:    components.sources.internal_metrics.output.metrics.sqs_message_receive_succeeded_total
		sqs_message_received_messages_total:    components.sources.internal_metrics.output.metrics.sqs_message_received_messages_total
		sqs_s3_event_record_ignored_total:      components.sources.internal_metrics.output.metrics.sqs_s3_event_record_ignored_total
	}
}
