package metadata

components: sinks: aws_kinesis_streams: components._aws & {
	title: "AWS Kinesis Data Streams"

	classes: {
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
		warnings: []
	}

	configuration: generated.components.sinks.aws_kinesis_streams.configuration & {
		_aws_include: false
	}

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: {
		aggregation: {
			title: "Record aggregation"
			body: """
				Enable `aggregation.enabled` to combine original logs into fewer Kinesis records.
				Aggregation is disabled by default. When enabled, it uses standard KPL framing.
				Select `aggregation.format: newline` explicitly for text/JSONL.
				Consumers must support the selected format. This option belongs to the
				Kinesis Streams sink, including streams consumed by Firehose; it does not
				change the direct Firehose sink.

				```yaml
				aggregation:
				  enabled: true
				  format: kpl
				  max_events: 500
				  max_bytes: 943718
				  timeout_secs: 1.0
				```

				`max_events` counts original logs. KPL accepts at most 1000 original logs per
				record. `max_bytes` includes newline delimiters or the KPL envelope. Newline
				counts uncompressed data; KPL counts the independently compressed user payloads
				and envelope. Random-key mode reserves the maximum possible key-table size.
				The timeout starts with the first buffered log.
				The existing `batch` options group completed Kinesis records into API requests;
				both stages can add buffering time.
				An underfilled record can wait for the aggregation timeout and then a new API
				batch timeout. A shorter `batch.timeout_secs`, such as `0.05`, reduces that
				additional API wait. It can also produce smaller API requests during sparse input.

				For newline mode, use text or compact JSON encoding. Each serialized log receives a trailing LF
				if it does not already have one. Text bytes, including embedded newlines, are
				preserved. Compression applies once to each completed aggregate. Consumers must
				decompress the record before splitting its lines.
				These text/JSON restrictions apply to newline mode only. KPL supports any
				configured log serializer, including binary formats, and preserves exact
				user payload boundaries without adding LF. Compression applies to each user
				payload before KPL framing; the KPL envelope itself remains uncompressed.

				For Firehose JSON or newline-delimiter deaggregation, use uncompressed
				compact JSON objects with `format: newline` and at most 500 objects per record.
				Do not use a JSON array. Firehose deaggregates KPL automatically when consuming
				from Kinesis Data Streams; KPL deaggregation alone does not decompress user
				payloads. Use uncompressed JSON for built-in Firehose JSON processing and
				configure destination compression in Firehose if required.

				The aggregator holds one active record; normal sink and request buffers also apply.
				With an explicit `partition_key_field`,
				only consecutive logs with the same key combine; a key change flushes the buffer.
				Changing keys for every log prevents aggregation and can reduce throughput.
				Without that field, a random key is generated for each completed record.
				KPL uses the same key for every inner user record and the outer Kinesis record.
				It does not combine unrelated explicit keys or implement shard-aware buffering.
				The sink enforces a conservative 1 MiB limit, including the actual partition-key
				bytes and KPL envelope where applicable. Newline checks before and after compression;
				KPL checks the complete framed record. It splits oversized aggregates at original
				log boundaries and rejects a singleton that cannot fit. Original log counts and
				acknowledgements are retained for every aggregate.

				KPL requires `USER_PARTITION_KEY` distribution. Startup calls
				`DescribeStreamSummary` even if healthchecks are disabled, requiring the
				`kinesis:DescribeStreamSummary` IAM permission. It rejects `AUTO` or unknown
				distribution values. Provisioned streams and older compatible endpoints can
				omit the distribution field. This is a startup check; do not change a running
				KPL producer's stream to `AUTO`. Service-managed distribution can cause KCL
				consumers to discard KPL records.
				"""
		}
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
