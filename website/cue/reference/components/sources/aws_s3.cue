package metadata

components: sources: aws_s3: components._aws & {
	title: "AWS S3"

	features: {
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
		development:   "beta"
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

	configuration: {
		acknowledgements: configuration._source_acknowledgements
		strategy: {
			common:      false
			description: "The strategy to use to consume objects from AWS S3."
			required:    false
			type: string: {
				default: "sqs"
				enum: {
					sqs: "Consume S3 objects by polling for bucket notifications sent to an [AWS SQS queue](\(urls.aws_sqs))."
				}
			}
		}
		compression: {
			common:      false
			description: "The compression format of the S3 objects.."
			required:    false
			type: string: {
				default: "text"
				enum: {
					auto: "Vector will try to determine the compression format of the object from its: `Content-Encoding` metadata, `Content-Type` metadata, and key suffix (e.g. `.gz`). It will fallback to 'none' if it cannot determine the compression."
					gzip: "GZIP format."
					zstd: "ZSTD format."
					none: "Uncompressed."
				}
			}
		}
		sqs: {
			common:      true
			description: "SQS strategy options. Required if strategy=`sqs`."
			required:    false
			type: object: {
				examples: []
				options: {
					poll_secs: {
						common:      true
						description: "How long to wait when polling SQS for new messages."
						required:    false
						type: uint: {
							default: 15
							unit:    "seconds"
						}
					}
					visibility_timeout_secs: {
						common:      false
						description: "The visibility timeout to use for messages in seconds. This controls how long a message is left unavailable when a Vector receives it. If a `vector` does not delete the message before the timeout expires, it will be made reavailable for another consumer; this can happen if, for example, the `vector` process crashes."
						required:    false
						warnings: ["Should be set higher than the length of time it takes to process an individual message to avoid that message being reprocessed."]
						type: uint: {
							default: 300
							unit:    "seconds"
						}
					}
					delete_message: {
						common:      true
						description: "Whether to delete the message once Vector processes it. It can be useful to set this to `false` to debug or during initial Vector setup."
						required:    false
						type: bool: default: true
					}
					queue_url: {
						description: "The URL of the SQS queue to receive bucket notifications from."
						required:    true
						type: string: {
							examples: ["https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue"]
						}
					}
				}
			}
		}
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
					_action:       "ReceiveMessage"
					required_when: "[`strategy`](#strategy) is set to `sqs`"
				},
				{
					_action:       "DeleteMessage"
					required_when: "[`strategy`](#strategy) is set to `sqs` and [`delete_message`](#sqs.delete_message) is set to `true`"
				},
			]
		},
	]

	telemetry: metrics: {
		events_in_total:                        components.sources.internal_metrics.output.metrics.events_in_total
		processed_bytes_total:                  components.sources.internal_metrics.output.metrics.processed_bytes_total
		component_discarded_events_total:       components.sources.internal_metrics.output.metrics.component_discarded_events_total
		component_errors_total:                 components.sources.internal_metrics.output.metrics.component_errors_total
		component_received_bytes_total:         components.sources.internal_metrics.output.metrics.component_received_bytes_total
		component_received_events_total:        components.sources.internal_metrics.output.metrics.component_received_events_total
		component_received_event_bytes_total:   components.sources.internal_metrics.output.metrics.component_received_event_bytes_total
		sqs_message_delete_failed_total:        components.sources.internal_metrics.output.metrics.sqs_message_delete_failed_total
		sqs_message_delete_succeeded_total:     components.sources.internal_metrics.output.metrics.sqs_message_delete_succeeded_total
		sqs_message_processing_failed_total:    components.sources.internal_metrics.output.metrics.sqs_message_processing_failed_total
		sqs_message_processing_succeeded_total: components.sources.internal_metrics.output.metrics.sqs_message_processing_succeeded_total
		sqs_message_receive_failed_total:       components.sources.internal_metrics.output.metrics.sqs_message_receive_failed_total
		sqs_message_receive_succeeded_total:    components.sources.internal_metrics.output.metrics.sqs_message_receive_succeeded_total
		sqs_message_received_messages_total:    components.sources.internal_metrics.output.metrics.sqs_message_received_messages_total
		sqs_s3_event_record_ignored_total:      components.sources.internal_metrics.output.metrics.sqs_s3_event_record_ignored_total
	}
}
