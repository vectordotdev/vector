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
					enum: ["json", "text", "parquet"]
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

		parquet_encoding: {
			title: "Parquet encoding"
			body:  """
				The AWS S3 sink supports encoding events in [Apache Parquet](\(urls.apache_parquet))
				format, which is a columnar storage format optimized for analytics workloads. Parquet
				provides efficient compression and encoding schemes, making it ideal for long-term
				storage and query performance with tools like AWS Athena, Apache Spark, and Presto.

				## Schema Configuration

				When using Parquet encoding, you **must** specify a schema that defines the structure and
				types of the Parquet file columns. The schema is defined as a simple map of field names to
				data types. Vector events are converted to Arrow RecordBatches and then written as Parquet files.

				All fields defined in the schema are nullable by default, meaning missing fields will be encoded
				as NULL values in the Parquet file.

				**Example configuration:**

				```yaml
				sinks:
				  s3:
				    type: aws_s3
				    bucket: my-bucket
				    compression: none  # Parquet handles compression internally
				    batch:
				      max_events: 50000
				      timeout_secs: 60
				    encoding:
				      codec: parquet
				      schema:
				        # Timestamps
				        timestamp: timestamp_microsecond
				        created_at: timestamp_millisecond

				        # String fields
				        user_id: utf8
				        event_name: utf8
				        message: utf8

				        # Numeric fields
				        team_id: int64
				        duration_ms: float64
				        count: int32

				        # Boolean
				        is_active: boolean

				      parquet:
				        compression: zstd
				        row_group_size: 50000  # Should be <= batch.max_events
				        allow_nullable_fields: true
				        estimated_output_size: 10485760  # 10MB - tune based on your data
				        enable_bloom_filters: true  # Enable for better query performance
				        bloom_filter_fpp: 0.05  # 5% false positive rate
				        bloom_filter_ndv: 1000000  # Expected distinct values
				```

				## Supported Data Types

				The following data types are supported for Parquet schema fields:

				**String types:**
				- `utf8` or `string`: UTF-8 encoded strings
				- `large_utf8` or `large_string`: Large UTF-8 strings (>2GB)

				**Integer types:**
				- `int8`, `int16`, `int32`, `int64`: Signed integers
				- `uint8`, `uint16`, `uint32`, `uint64`: Unsigned integers

				**Floating point types:**
				- `float32` or `float`: 32-bit floating point
				- `float64` or `double`: 64-bit floating point

				**Timestamp types:**
				- `timestamp_second` or `timestamp_s`: Seconds since Unix epoch
				- `timestamp_millisecond`, `timestamp_ms`, or `timestamp_millis`: Milliseconds since Unix epoch
				- `timestamp_microsecond`, `timestamp_us`, or `timestamp_micros`: Microseconds since Unix epoch
				- `timestamp_nanosecond`, `timestamp_ns`, or `timestamp_nanos`: Nanoseconds since Unix epoch

				**Date types:**
				- `date32` or `date`: Days since Unix epoch (32-bit)
				- `date64`: Milliseconds since Unix epoch (64-bit)

				**Other types:**
				- `boolean` or `bool`: Boolean values
				- `binary`: Arbitrary binary data
				- `large_binary`: Large binary data (>2GB)
				- `decimal128`: 128-bit decimal with default precision
				- `decimal256`: 256-bit decimal with default precision

				## Parquet Configuration Options

				### compression

				Compression algorithm applied to Parquet column data:
				- `snappy` (default): Fast compression with moderate compression ratio
				- `gzip`: Balanced compression, excellent AWS Athena compatibility
				- `zstd`: Best compression ratio, ideal for cold storage
				- `lz4`: Very fast compression, good for high-throughput scenarios
				- `brotli`: Good compression, web-optimized
				- `uncompressed`: No compression

				### row_group_size

				Number of rows per row group in the Parquet file. Row groups are Parquet's unit of
				parallelization - query engines can read different row groups in parallel.

				**Important:** Since each batch becomes a separate Parquet file, `row_group_size` should
				be less than or equal to `batch.max_events`. Row groups cannot span multiple files.
				If omitted, defaults to the batch size.

				**Trade-offs:**
				- **Larger row groups** (500K-1M rows): Better compression, less query parallelism
				- **Smaller row groups** (50K-100K rows): More query parallelism, slightly worse compression

				For AWS Athena, row groups of 128-256 MB (uncompressed) are often recommended.

				### allow_nullable_fields

				When enabled, missing or incompatible values will be encoded as NULL even for fields that
				would normally be non-nullable. This is useful when working with downstream systems that
				can handle NULL values through defaults or computed columns.

				### estimated_output_size

				Estimated compressed output size in bytes for buffer pre-allocation. This is an optional
				performance tuning parameter that can significantly reduce memory overhead by pre-allocating
				the output buffer to an appropriate size, avoiding repeated reallocations during encoding.

				**How to set this value:**
				1. Monitor actual compressed Parquet file sizes in production
				2. Set to approximately 1.2x your average observed compressed size for headroom
				3. ZSTD compression typically achieves 3-10x compression on JSON/log data

				**Example:** If your batches are 100MB uncompressed and compress to 10MB on average,
				set `estimated_output_size: 12582912` (12MB) to provide some headroom.

				If not specified, Vector uses a heuristic based on estimated uncompressed size
				(approximately 2KB per event, capped at 128MB).

				**Trade-offs:**
				- **Too small**: Minimal benefit, will still require reallocations
				- **Too large**: Wastes memory by over-allocating
				- **Just right**: Optimal memory usage with minimal reallocations

				### enable_bloom_filters

				Enable Bloom filters for all columns in the Parquet file. Bloom filters are probabilistic
				data structures that can significantly improve query performance by allowing query engines
				(like AWS Athena, Apache Spark, and Presto) to skip entire row groups when searching for
				specific values without reading the actual data.

				**When to enable:**
				- High-cardinality columns: UUIDs, user IDs, session IDs, transaction IDs
				- String columns frequently used in WHERE clauses: URLs, emails, tags, names
				- Point queries: `WHERE user_id = 'abc123'`
				- IN clause queries: `WHERE id IN ('x', 'y', 'z')`

				**Trade-offs:**
				- **Pros**: Significantly faster queries, better row group pruning, reduced I/O
				- **Cons**: Slightly larger file sizes (typically 1-5% overhead), minimal write overhead

				**Default**: `false` (disabled)

				### bloom_filter_fpp

				False positive probability (FPP) for Bloom filters. This controls the trade-off between
				Bloom filter size and accuracy. Lower values produce larger but more accurate filters.

				- **Default**: `0.05` (5% false positive rate)
				- **Range**: Must be between 0.0 and 1.0 (exclusive)
				- **Recommended values**:
				  - `0.05` (5%): Good balance for general use
				  - `0.01` (1%): Better for high-selectivity queries where precision matters
				  - `0.10` (10%): Smaller filters when storage is a concern

				A false positive means the Bloom filter indicates a value *might* be in a row group when it
				actually isn't, requiring the engine to read and filter that row group. Lower FPP means fewer
				unnecessary reads.

				Only takes effect when `enable_bloom_filters` is `true`.

				### bloom_filter_ndv

				Estimated number of distinct values (NDV) for Bloom filter sizing. This should match the
				expected cardinality of your columns. Higher values result in larger Bloom filters.

				- **Default**: `1,000,000`
				- **Recommendation**: Analyze your data to determine actual cardinality
				  - Low cardinality (countries, states): `1,000` - `100,000`
				  - Medium cardinality (cities, products): `100,000` - `1,000,000`
				  - High cardinality (user IDs, UUIDs): `10,000,000+`

				**Important**: If your actual distinct value count significantly exceeds this number, the
				false positive rate may increase beyond the configured `bloom_filter_fpp`, reducing query
				performance gains.

				Only takes effect when `enable_bloom_filters` is `true`.

				## Batching Behavior

				Each batch of events becomes **one Parquet file** in S3. The batch size is controlled by:
				- `batch.max_events`: Maximum number of events per file
				- `batch.max_bytes`: Maximum bytes per file
				- `batch.timeout_secs`: Maximum time to wait before flushing

				Example: With `max_events: 50000`, each Parquet file will contain up to 50,000 rows.

				## Important Notes

				- **Sink-level compression**: Set `compression: none` at the sink level since Parquet
				  handles compression internally through its `parquet.compression` setting
				- **All fields nullable**: Fields defined in the schema are nullable by default, allowing
				  for missing values
				- **Schema required**: The schema cannot be inferred and must be explicitly configured
				- **AWS Athena compatibility**: Use `gzip` compression for best Athena compatibility
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
