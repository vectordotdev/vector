package metadata

components: sinks: datadog_archives: {
	title: "Datadog Log Archives"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["AWS"] // GCP, Azure is coming
		stateful:                  false
	}

	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      false
				common:       false
				timeout_secs: 0
			}
			compression: enabled: false
			encoding: enabled:    false
			proxy: enabled:       false
			request: {
				enabled:        true
				concurrency:    50
				rate_limit_num: 250
				headers:        false
			}
			tls: enabled: false
		}
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":      true
			"aarch64-unknown-linux-musl":     true
			"armv7-unknown-linux-gnueabihf":  true
			"armv7-unknown-linux-musleabihf": true
			"x86_64-apple-darwin":            true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		bucket: {
			description: "The S3 bucket name. Do not include a leading `s3://` or a trailing `/`."
			required:    true
			warnings: []
			type: string: {
				examples: ["my-bucket"]
				syntax: "literal"
			}
		}
		key_prefix: {
			common:      true
			category:    "File Naming"
			description: "A prefix to apply to all object key names. This should be used to partition your objects in \"folders\"."
			required:    false
			warnings: []
			type: string: {
				default: "/"
				examples: ["logs/audit"]
				syntax: "literal"
			}
		}
		service: {
			category:    "Storage"
			description: "An external storage service where archived logs are sent to."
			required:    true
			warnings: []
			type: string: {
				enum: {
					aws_s3: "[AWS S3](\(urls.aws_s3)) is used as an external storage service."
				}
				syntax: "literal"
			}
		}
		aws_s3: {
			description:   "AWS S3 specific configuration options. Required when `service` has the value `\"aws_s3\"`."
			common:        false
			required:      false
			relevant_when: "service = \"aws_s3\""
			warnings: []
			type: object: {
				examples: []
				options: {
					auth: {
						common:      false
						description: "Options for the authentication strategy. Check the [`auth`](\(urls.vector_aws_s3_sink_auth)) section of the AWS S3 sink for more details."
						required:    false
						warnings: []
						type: object: {}
					}
					acl:                    sinks.aws_s3.configuration.acl
					grant_full_control:     sinks.aws_s3.configuration.grant_full_control
					grant_read:             sinks.aws_s3.configuration.grant_read
					grant_read_acp:         sinks.aws_s3.configuration.grant_read_acp
					grant_write_acp:        sinks.aws_s3.configuration.grant_write_acp
					server_side_encryption: sinks.aws_s3.configuration.server_side_encryption
					ssekms_key_id:          sinks.aws_s3.configuration.ssekms_key_id
					storage_class: {
						category: "Storage"
						common:   false
						description: """
          			The storage class for the created objects. See [the S3 Storage Classes](https://docs.aws.amazon.com/AmazonS3/latest/dev/storage-class-intro.html) for more details.
          			Log Rehydration supports all storage classes except for Glacier and Glacier Deep Archive.
          			"""
						required: false
						warnings: []
						type: string: {
							default: null
							enum: {
								STANDARD:            "The default storage class. If you don't specify the storage class when you upload an object, Amazon S3 assigns the STANDARD storage class."
								REDUCED_REDUNDANCY:  "Designed for noncritical, reproducible data that can be stored with less redundancy than the STANDARD storage class. AWS recommends that you not use this storage class. The STANDARD storage class is more cost effective. "
								INTELLIGENT_TIERING: "Stores objects in two access tiers: one tier that is optimized for frequent access and another lower-cost tier that is optimized for infrequently accessed data."
								STANDARD_IA:         "Amazon S3 stores the object data redundantly across multiple geographically separated Availability Zones (similar to the STANDARD storage class)."
								ONEZONE_IA:          "Amazon S3 stores the object data in only one Availability Zone."
							}
							syntax: "literal"
						}
					}
					tags: sinks.aws_s3.configuration.tags
					region: {
						description: "The [AWS region](\(urls.aws_regions)) of the target service."
						required:    true
						type: string: {
							examples: ["us-east-1"]
							syntax: "literal"
						}
					}
				}
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	how_it_works: {

		a_object_key_format: {
			title: "Custom object key format"
			body: """
				Objects written to the external archives have the following key format:
				```text
				/my/bucket/my/key/prefix/dt=<YYYYMMDD>/hour=<HH>/<UUID>.json.gz
				```
				For example:

				```text
				/my/bucket/my/key/prefix/dt=20180515/hour=14/7dq1a9mnSya3bFotoErfxl.json.gz
				```
				"""
		}

		b_event_preprocessing: {
			title: "Event format/pre-processing"
			body:  """
				Within the gzipped JSON file, each event’s content is formatted as follows:

				```json
				{
				  "_id": "123456789abcdefg",
				  "date": "2018-05-15T14:31:16.003Z",
				  "host": "i-12345abced6789efg",
				  "source": "source_name",
				  "service": "service_name",
				  "status": "status_level",
				  "message": "2018-05-15T14:31:16.003Z INFO rid='acb-123' status=403 method=PUT",
				  "attributes": { "rid": "abc-123", "http": { "status_code": 403, "method": "PUT" } },
				  "tags": [ "env:prod", "team:acme" ]
				}
				```

				Events are pre-processed as follows:

				- `_id` is always generated in the sink
				- `date` is set from the Global [Log Schema](\(urls.vector_log_schema))'s `timestamp_key` mapping,
				or to the current time if missing
				- `message`,`host` are also set from the corresponding Global [Log Schema](\(urls.vector_log_schema)) mappings
				- `source`, `service`, `status`, `tags` are left as is
				- the rest of the fields is moved to `attributes`

				Though only `_id` and `date` are mandatory,
				most reserved attributes( `host`, `source`, `service`, `status`, `message`, `tags`) are expected
				for a meaningful log processing by DataDog. Therefore users should make sure that these optional fields are populated
				before they reach this sink.
				"""
		}

		c_aws: {
			title: "AWS S3 setup"
			body:  """
				For more details about AWS S3 configuration and how it works check out [AWS S3 sink](\(urls.vector_aws_s3_sink_how_it_works)).
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
					_action: "HeadBucket"
					required_for: ["healthcheck"]
				},
				{
					_action: "PutObject"
				},
			]
		},
	]

	telemetry: metrics: {
		events_discarded_total:  components.sources.internal_metrics.output.metrics.events_discarded_total
		processing_errors_total: components.sources.internal_metrics.output.metrics.processing_errors_total
	}
}
