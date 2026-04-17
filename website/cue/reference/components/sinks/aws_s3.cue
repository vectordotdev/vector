package metadata

components: sinks: aws_s3: components._aws & {
	title: "AWS S3"

	classes: {
		commonly_used: true
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
				common:       true
				max_bytes:    10000000
				timeout_secs: 300.0
			}
			compression: {
				enabled: true
				default: "gzip"
				algorithms: ["none", "gzip"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: {
				enabled: true
				codec: {
					enabled: true
					framing: true
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
				service: services.aws_s3

				interface: {
					socket: {
						api: {
							title: "AWS S3 API"
							url:   urls.aws_s3_endpoints
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

	configuration: generated.components.sinks.aws_s3.configuration & {
		_aws_include: false
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
		traces: true
	}

	how_it_works: {
		cross_account: {
			title: "Cross account object writing"
			body:  """
				If you're using Vector to write objects across AWS accounts then you should
				consider setting the `grant_full_control` option to the bucket owner's
				canonical user ID. AWS provides a
				[full tutorial](\(urls.aws_s3_cross_account_tutorial)) for this use case. If
				don't know the bucket owner's canonical ID you can find it by following
				[this tutorial](\(urls.aws_canonical_user_id)).
				"""
		}

		log_on_put: {
			title: "Emit a log when putting an object"
			body: """
				If you're using Vector to write objects to an s3-compatible storage, you can
				set `VECTOR_LOG` to `vector::sinks::s3_common::service::put_object=trace` to
				enable a trace log containing the bucket and key the object was put to. This
				is best used when writing an object to an s3-compatible storage to kick off
				post-put operations through another sink.
				"""
		}

		object_acl: {
			title: "Object Access Control List (ACL)"
			body:  """
				AWS S3 supports [access control lists (ACL)](\(urls.aws_s3_acl)) for buckets and
				objects. In the context of Vector, only object ACLs are relevant (Vector does
				not create or modify buckets). You can set the object level ACL by using one
				of the `acl`, `grant_full_control`, `grant_read`, `grant_read_acp`, or
				`grant_write_acp` options.
				"""
			sub_sections: [
				{
					title: "`acl.*` vs `grant_*` options"
					body:  """
						The `grant_*` options name a specific entity to grant access to. The `acl`
						options is one of a set of [specific canned ACLs](\(urls.aws_s3_canned_acl)) that
						can only name the owner or world.
						"""
				},
			]
		}

		object_naming: {
			title: "Object naming"
			body:  """
				Vector uses two different naming schemes for S3 objects. If you set the
				[`compression`](#compression) parameter to `true` (this is the default), Vector uses
				this scheme:

				```text
				<key_prefix><timestamp>-<uuidv4>.log.gz
				```

				If compression isn't enabled, Vector uses this scheme (only the file extension
				is different):

				```text
				<key_prefix><timestamp>-<uuidv4>.log
				```

				Some sample S3 object names (with and without compression, respectively):

				```text
				date=2019-06-18/1560886634-fddd7a0e-fad9-4f7e-9bce-00ae5debc563.log.gz
				date=2019-06-18/1560886634-fddd7a0e-fad9-4f7e-9bce-00ae5debc563.log
				```

				Vector appends a [UUIDV4](\(urls.uuidv4)) token to ensure there are no naming
				conflicts in the unlikely event that two Vector instances are writing data at the
				same time.

				You can control the resulting name via the [`key_prefix`](#key_prefix),
				[`filename_time_format`](#filename_time_format), and
				[`filename_append_uuid`](#filename_append_uuid) options.

				For example, to store objects at the root S3 folder, without a timestamp or UUID use
				these configuration options:

				```text
				key_prefix = "{{ my_file_name }}"
				filename_time_format = ""
				filename_append_uuid = false
				```
				"""
		}

		object_tags_and_metadata: {
			title: "Object Tags & metadata"
			body:  """
				Vector currently only supports [AWS S3 object tags](\(urls.aws_s3_tags)) and does
				_not_ support [object metadata](\(urls.aws_s3_metadata)). If you require metadata
				support see [issue #1694](\(urls.issue_1694)).

				We believe tags are more flexible since they are separate from the actual S3
				object. You can freely modify tags without modifying the object. Conversely,
				object metadata requires a full rewrite of the object to make changes.
				"""
		}

		server_side_encryption: {
			title: "Server-Side Encryption (SSE)"
			body:  """
				AWS S3 offers [server-side encryption](\(urls.aws_s3_sse)). You can apply defaults
				at the bucket level or set the encryption at the object level. In the context,
				of Vector only the object level is relevant (Vector does not create or modify
				buckets). Although, we recommend setting defaults at the bucket level when
				possible. You can explicitly set the object level encryption via the
				`server_side_encryption` option.
				"""
		}

		storage_class: {
			title: "Storage class"
			body:  """
				AWS S3 offers [storage classes](\(urls.aws_s3_storage_classes)). You can apply
				defaults, and rules, at the bucket level or set the storage class at the object
				level. In the context of Vector only the object level is relevant (Vector does
				not create or modify buckets). You can set the storage class via the
				`storage_class` option.
				"""
		}

		parquet_encoding: {
			title: "Parquet Batch Encoding"
			body: """
				The S3 sink supports Apache Parquet batch encoding with the `batch_encoding`
				option. When configured, events are encoded together as Parquet columnar files
				instead of the default per-event JSON or text encoding. Parquet files are
				optimized for analytical queries using Athena, Trino, Spark, and other columnar
				query engines.

				Parquet handles compression internally at the column page level, so the
				top-level `compression` setting must be set to `"none"`.

				Output files automatically use the `.parquet` extension.

				This feature requires the `codecs-parquet` feature flag at compile time.

				There are two ways to provide a schema: supply a `schema_file`, or set
				`schema_mode` to `auto_infer` and let Vector derive the schema from each
				incoming batch.

				#### Option 1: Schema file

				Load the schema from a native Parquet `.schema` file. The file must
				contain a valid Parquet message type definition.

				```toml
				[sinks.s3_parquet]
				type = "aws_s3"
				inputs = ["my-source"]
				bucket = "my-analytics-bucket"
				key_prefix = "logs/date=%F"
				compression = "none"

				[sinks.s3_parquet.encoding]
				codec = "text"

				[sinks.s3_parquet.batch_encoding]
				codec = "parquet"
				schema_file = "/etc/vector/schemas/logs.schema"
				schema_mode = "relaxed"

				[sinks.s3_parquet.batch_encoding.compression]
				algorithm = "snappy"
				```

				#### Option 2: Auto-infer schema

				Vector infers the Arrow schema from the fields present in each batch.
				`Value::Timestamp` fields are automatically promoted to
				`Timestamp(Microsecond, UTC)`. No schema file is required.

				```toml
				[sinks.s3_parquet]
				type = "aws_s3"
				inputs = ["my-source"]
				bucket = "my-analytics-bucket"
				key_prefix = "logs/date=%F"
				compression = "none"

				[sinks.s3_parquet.encoding]
				codec = "text"

				[sinks.s3_parquet.batch_encoding]
				codec = "parquet"
				schema_mode = "auto_infer"

				[sinks.s3_parquet.batch_encoding.compression]
				algorithm = "snappy"
				```

				#### YAML example

				```yaml
				sinks:
				  s3_parquet:
				    type: aws_s3
				    inputs:
				      - my-source
				    bucket: my-analytics-bucket
				    key_prefix: "logs/date=%F"
				    compression: none
				    encoding:
				      codec: text
				    batch_encoding:
				      codec: parquet
				      schema_mode: auto_infer
				      compression:
				        algorithm: gzip
				        level: 9
				```

				#### Configuration reference

				| Field | Type | Required | Description |
				|---|---|---|---|
				| `codec` | string | yes | Must be `"parquet"` |
				| `schema_file` | path | no | Path to a native Parquet `.schema` file. Required when `schema_mode` is `relaxed` or `strict`. |
				| `schema_mode` | string | no | `relaxed` (default), `strict`, or `auto_infer`. See the section on schema_mode values. |
				| `compression` | object | no | Column-level compression. See the section on compression options. |

				#### `schema_mode` values

				| Value | Description |
				|---|---|
				| `relaxed` (default) | Missing schema fields become null. Extra event fields are silently dropped. |
				| `strict` | Missing schema fields become null. Extra event fields cause an encoding error. |
				| `auto_infer` | Schema is inferred from each batch. No `schema_file` needed. `Value::Timestamp` fields are promoted to `Timestamp(Microsecond, UTC)`. |

				#### Compression options

				Compression is configured as a nested object with an `algorithm` key.
				Algorithms that support levels accept an additional `level` key.

				| Algorithm | Level range | Default |
				|---|---|---|
				| `snappy` | — | yes |
				| `zstd` | 1–21 | — |
				| `gzip` | 1–9 | — |
				| `lz4` | — | — |
				| `none` | — | — |

				#### Unsupported types

				Binary fields are rejected at config time because the internal Arrow JSON
				encoder cannot materialize them. Use `utf8` with base64 or hex encoding
				for binary data instead.
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
					_action: "ListBucket"
					required_for: ["healthcheck"]
				},
				{
					_action: "PutObject"
				},
			]
		},
	]
}
