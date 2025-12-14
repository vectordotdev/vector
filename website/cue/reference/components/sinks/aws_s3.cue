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

				Vector supports two approaches for defining the Parquet schema:

				1. **Explicit Schema**: Define the exact structure and data types for your Parquet files
				2. **Automatic Schema Inference**: Let Vector automatically infer the schema from your event data

				You must choose exactly one approach - they are mutually exclusive.

				### Automatic Schema Inference (Recommended for Getting Started)

				When enabled, Vector automatically infers the schema from each batch of events by examining
				the data types of values in the events. This is the easiest way to get started with Parquet
				encoding.

				**Type mapping:**
				- String values → `utf8`
				- Integer values → `int64`
				- Float values → `float64`
				- Boolean values → `boolean`
				- Timestamp values → `timestamp_microsecond`
				- Arrays/Objects → `utf8` (serialized as JSON)

				**Type conflicts:** If a field has different types across events in the same batch,
				it will be encoded as `utf8` (string) and all values will be converted to strings.

				**Important:** Schema consistency across batches is the operator's responsibility.
				Use VRL transforms to ensure consistent types if needed. Each batch may produce
				a different schema if event structure varies.

				**Limitations:** Bloom filters and sorting are not supported with automatic schema inference.
				Use explicit schema if you need these features.

				**Example configuration with schema inference:**

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
				      parquet:
				        infer_schema: true
				        exclude_columns:
				          - _metadata
				          - internal_id
				        max_columns: 1000
				        compression: zstd
				        compression_level: 6
				        writer_version: v2
				        row_group_size: 50000
				```

				### Explicit Schema (Recommended for Production)

				For production use, explicitly defining the schema provides better control, consistency,
				and access to advanced features like per-column Bloom filters and sorting. The schema
				is defined as a map of field names to field definitions.

				All fields defined in the schema are nullable by default, meaning missing fields will be encoded
				as NULL values in the Parquet file.

				**Example configuration with explicit schema:**

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
				      parquet:
				        schema:
				          # Timestamps
				          timestamp:
				            type: timestamp_microsecond
				            bloom_filter: false
				          created_at:
				            type: timestamp_millisecond
				            bloom_filter: false

				          # String fields with per-column Bloom filters
				          user_id:
				            type: utf8
				            bloom_filter: true  # Enable for high-cardinality field
				            bloom_filter_num_distinct_values: 10000000
				            bloom_filter_false_positive_pct: 0.01
				          event_name:
				            type: utf8
				            bloom_filter: false
				          message:
				            type: utf8
				            bloom_filter: false

				          # Numeric fields
				          team_id:
				            type: int64
				            bloom_filter: false
				          duration_ms:
				            type: float64
				            bloom_filter: false
				          count:
				            type: int32
				            bloom_filter: false

				          # Boolean
				          is_active:
				            type: boolean
				            bloom_filter: false

				        compression: zstd
				        compression_level: 6  # ZSTD level 1-22 (higher = better compression)
				        writer_version: v2  # Use modern Parquet format
				        row_group_size: 50000  # Should be <= batch.max_events
				        allow_nullable_fields: true
				        sorting_columns:  # Pre-sort for better compression and queries
				          - column: timestamp
				            descending: true  # Most recent first
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

				### Schema Options

				#### schema

				Explicitly define the Arrow schema for encoding events to Parquet. This schema defines
				the structure and types of the Parquet file columns, specified as a map of field names
				to field definitions.

				Each field definition includes:
				- **type**: The Arrow data type (required)
				- **bloom_filter**: Enable Bloom filter for this column (optional, default: false)
				- **bloom_filter_num_distinct_values**: Number of distinct values for this column's Bloom filter (optional)
				- **bloom_filter_false_positive_pct**: False positive probability for this column's Bloom filter (optional)

				All fields are nullable by default, meaning missing fields will be encoded as NULL values.

				**Mutually exclusive with `infer_schema`**. You must specify either `schema` or
				`infer_schema: true`, but not both.

				**Example:**
				```yaml
				schema:
				  user_id:
				    type: utf8
				    bloom_filter: true
				    bloom_filter_num_distinct_values: 10000000
				    bloom_filter_false_positive_pct: 0.01
				  timestamp:
				    type: timestamp_microsecond
				    bloom_filter: false
				  count:
				    type: int64
				    bloom_filter: false
				```

				#### infer_schema

				Automatically infer the schema from event data. When enabled, Vector examines each
				batch of events and automatically determines the appropriate Arrow data types based
				on the values present.

				**Type inference rules:**
				- String values → `utf8`
				- Integer values → `int64`
				- Float values → `float64`
				- Boolean values → `boolean`
				- Timestamp values → `timestamp_microsecond`
				- Arrays/Objects → `utf8` (serialized as JSON)
				- Type conflicts → `utf8` (fallback to string with warning)

				**Important considerations:**
				- Schema may vary between batches if event structure changes
				- Use VRL transforms to ensure type consistency if needed
				- Bloom filters and sorting are not available with inferred schemas
				- For production workloads, explicit schemas are recommended

				**Mutually exclusive with `schema`**. You must specify either `schema` or
				`infer_schema: true`, but not both.

				**Default**: `false`

				#### exclude_columns

				Column names to exclude from Parquet encoding when using automatic schema inference.
				These columns will be completely excluded from the Parquet file.

				Useful for filtering out metadata, internal fields, or temporary data that shouldn't
				be persisted to long-term storage.

				**Only applies when `infer_schema` is enabled**. Ignored when using explicit schema
				(use the schema definition to control which fields are included).

				**Example:**
				```yaml
				infer_schema: true
				exclude_columns:
				  - _metadata
				  - internal_id
				  - temp_field
				```

				#### max_columns

				Maximum number of columns to encode when using automatic schema inference. Additional
				columns beyond this limit will be silently dropped. Columns are selected in the order
				they appear in the first event.

				This protects against accidentally creating Parquet files with too many columns, which
				can cause performance issues in query engines.

				**Only applies when `infer_schema` is enabled**. Ignored when using explicit schema.

				**Default**: `1000`

				**Recommended values:**
				- Standard use cases: `1000` (default)
				- Wide tables: `500` - `1000`
				- Performance-critical: `100` - `500`

				### Compression Options

				#### compression

				Compression algorithm applied to Parquet column data:
				- `snappy` (default): Fast compression with moderate compression ratio
				- `gzip`: Balanced compression, excellent AWS Athena compatibility
				- `zstd`: Best compression ratio, ideal for cold storage
				- `lz4`: Very fast compression, good for high-throughput scenarios
				- `brotli`: Good compression, web-optimized
				- `uncompressed`: No compression

				### compression_level

				Compression level for algorithms that support it (ZSTD, GZIP, Brotli). This controls the
				trade-off between compression ratio and encoding speed.

				**ZSTD levels (1-22):**
				- **1-3**: Fastest encoding, moderate compression (level 3 is default)
				- **4-9**: Good balance of speed and compression
				- **10-15**: Better compression, slower encoding (recommended for cold storage)
				- **16-22**: Maximum compression, slowest encoding

				**GZIP levels (1-9):**
				- **1-3**: Faster encoding, less compression
				- **6**: Default balance (recommended)
				- **9**: Maximum compression, slowest

				**Brotli levels (0-11):**
				- **0-4**: Faster encoding
				- **1**: Default (recommended)
				- **5-11**: Better compression, slower

				Higher levels typically produce 20-50% smaller files but take 2-5x longer to encode.
				**Recommendation:** Use level 3-6 for hot data, 10-15 for cold storage.

				### writer_version

				Parquet format version to write. Controls compatibility vs. performance.

				**Options:**
				- **v1** (default): PARQUET_1_0 - Maximum compatibility with older readers
				- **v2**: PARQUET_2_0 - Modern format with better encoding and statistics

				**Version 2 benefits:**
				- 10-20% more efficient encoding for certain data types
				- Better statistics for query optimization
				- Improved data page format
				- Required for some advanced features

				**When to use:**
				- Use **v1** for maximum compatibility with pre-2018 tools
				- Use **v2** for better performance with modern query engines (Athena, Spark, Presto)

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

				### Per-Column Bloom Filters

				Bloom filters are probabilistic data structures that can significantly improve query
				performance by allowing query engines (like AWS Athena, Apache Spark, and Presto) to
				skip entire row groups when searching for specific values without reading the actual data.

				**Only available when using explicit schema** (not available with automatic schema inference).

				When using an explicit schema, you can enable Bloom filters on a per-column basis
				by setting `bloom_filter: true` in the field definition. This gives you fine-grained
				control over which columns get Bloom filters.

				**When to use Bloom filters:**
				- High-cardinality columns: UUIDs, user IDs, session IDs, transaction IDs
				- String columns frequently used in WHERE clauses: URLs, emails, tags, names
				- Point queries: `WHERE user_id = 'abc123'`
				- IN clause queries: `WHERE id IN ('x', 'y', 'z')`

				**When NOT to use Bloom filters:**
				- Low-cardinality columns (countries, status codes, boolean flags)
				- Columns rarely used in WHERE clauses
				- Range queries (Bloom filters don't help with `>`, `<`, `BETWEEN`)

				**Trade-offs:**
				- **Pros**: Significantly faster queries (often 10-100x), better row group pruning, reduced I/O
				- **Cons**: Slightly larger file sizes (typically 1-5% overhead), minimal write overhead

				**Configuration example:**

				```yaml
				schema:
				  user_id:
				    type: utf8
				    bloom_filter: true              # Enable for high-cardinality column
				    bloom_filter_num_distinct_values: 10000000      # Expected distinct values
				    bloom_filter_false_positive_pct: 0.01          # 1% false positive rate
				  event_name:
				    type: utf8
				    bloom_filter: false             # Skip for low-cardinality column
				  timestamp:
				    type: timestamp_microsecond
				    bloom_filter: false             # Skip for timestamp (use sorting instead)
				```

				**Per-column Bloom filter settings:**

				- **bloom_filter**: Enable Bloom filter for this column (default: `false`)
				- **bloom_filter_num_distinct_values**: Expected number of distinct values for this column's Bloom filter
				  - Low cardinality (countries, states): `1,000` - `100,000`
				  - Medium cardinality (cities, products): `100,000` - `1,000,000`
				  - High cardinality (user IDs, UUIDs): `10,000,000+`
				  - If not specified, defaults to `1,000,000`
				  - Automatically capped to the `row_group_size` value
				- **bloom_filter_false_positive_pct**: False positive probability for this column's Bloom filter
				  - `0.05` (5%): Good balance for general use
				  - `0.01` (1%): Better for high-selectivity queries where precision matters
				  - `0.10` (10%): Smaller filters when storage is a concern
				  - If not specified, defaults to `0.05`

				A false positive means the Bloom filter indicates a value *might* be in a row group when it
				actually isn't, requiring the engine to read and filter that row group. Lower FPP means fewer
				unnecessary reads but larger Bloom filters.

				### sorting_columns

				Pre-sort rows by specified columns before writing to Parquet. This can significantly improve
				both compression ratios and query performance, especially for time-series data and event logs.

				**Benefits:**
				- **20-40% better compression**: Similar values are grouped together, improving compression
				- **Faster queries**: More effective min/max statistics enable better row group skipping
				- **Improved caching**: Query engines can cache sorted data more efficiently

				**Common patterns:**
				- **Time-series data**: Sort by `timestamp` descending (most recent first)
				- **Multi-tenant systems**: Sort by `tenant_id`, then `timestamp`
				- **User analytics**: Sort by `user_id`, then `event_time`
				- **Logs**: Sort by `timestamp`, then `severity`

				**Configuration:**
				```yaml
				sorting_columns:
				  - column: timestamp
				    descending: true   # Most recent first
				  - column: user_id
				    descending: false  # A-Z order
				```

				**Trade-offs:**
				- **Write performance**: Adds 10-30% sorting overhead during encoding
				- **Memory usage**: Requires buffering entire batch in memory for sorting
				- **Most beneficial**: When queries frequently filter on sorted columns

				**When to use:**
				- Enable for time-series data where you query recent events frequently
				- Enable for multi-tenant data partitioned by tenant_id
				- Skip if write latency is critical and queries don't benefit from sorting

				If not specified, rows are written in the order they appear in the batch.

				## Batching Behavior

				Each batch of events becomes **one Parquet file** in S3. The batch size is controlled by:
				- `batch.max_events`: Maximum number of events per file
				- `batch.max_bytes`: Maximum bytes per file
				- `batch.timeout_secs`: Maximum time to wait before flushing

				Example: With `max_events: 50000`, each Parquet file will contain up to 50,000 rows.

				## Important Notes

				- **Sink-level compression**: Set `compression: none` at the sink level since Parquet
				  handles compression internally through its `parquet.compression` setting
				- **Schema configuration**: You must choose either explicit schema or automatic schema
				  inference (`infer_schema: true`). For production use, explicit schemas are recommended
				  for consistency and access to advanced features like Bloom filters and sorting
				- **All fields nullable**: Fields defined in explicit schemas are nullable by default,
				  allowing for missing values. Inferred schemas also create nullable fields
				- **AWS Athena compatibility**: Use `gzip` or `snappy` compression for best Athena compatibility
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
