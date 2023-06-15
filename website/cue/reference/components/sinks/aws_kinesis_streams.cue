package metadata

components: sinks: aws_kinesis_streams: components._aws & {
	title: "AWS Kinesis Data Streams"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["AWS"]
		stateful: false
	}

	features: {
		acknowledgements: true
		auto_generated:   true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    5000000
				max_events:   500
				timeout_secs: 1.0
			}
			compression: {
				enabled: true
				default: "none"
				algorithms: ["none", "gzip"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: {
				enabled: true
				codec: {
					enabled: true
					enum: ["json", "text"]
				}
			}
			proxy: enabled: true
			request: {
				enabled: true
				headers: false
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
				enabled_by_scheme:      true
			}
			to: {
				service: services.aws_kinesis_data_streams

				interface: {
					socket: {
						api: {
							title: "AWS Kinesis Data Streams API"
							url:   urls.aws_kinesis_streams_api
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "required"
					}
				}
			}
		}
	}

	support: {
		requirements: []
		notices: []
		warnings: []
	}

	configuration: base.components.sinks.aws_kinesis_streams.configuration & {
		_aws_include: false
	}

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: {
		partitioning: {
			title: "Partitioning"
			body:  """
				By default, Vector issues random 16 byte values for each
				[Kinesis record's partition key](\(urls.aws_kinesis_partition_key)), evenly
				distributing records across your Kinesis partitions. Depending on your use case
				this might not be sufficient since random distribution does not preserve order.
				To override this, you can supply the `partition_key_field` option. This option
				presents an alternate field on your event to use as the partition key value instead.
				This is useful if you have a field already on your event, and it also pairs
				nicely with the [`remap` transform](\(urls.vector_remap_transform)), which enables you
				to add partition-related metadata to events.
				"""
			sub_sections: [
				{
					title: "Missing partition keys"
					body:  """
						Kinesis requires a value for the partition key. If the key is missing or the
						value is blank, the event is dropped and a
						[`warning`-level log event](\(urls.vector_monitoring)) is logged. The field
						specified in the `partition_key_field` option should thus always contain a
						value.
						"""
				},
				{
					title: "Partition keys that exceed 256 characters"
					body: """
						If the value provided exceeds the maximum allowed length of 256 characters
						Vector will slice the value and use the first 256 characters.
						"""
				},
				{
					title: "Non-string partition keys"
					body: """
						Vector will coerce the value into a string.
						"""
				},
			]
		}
	}

	permissions: iam: [
		{
			platform: "aws"
			_service: "kinesis"

			policies: [
				{
					_action: "DescribeStream"
					required_for: ["healthcheck"]
				},
				{
					_action: "PutRecords"
				},
			]
		},
	]
}
