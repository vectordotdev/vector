package metadata

base: components: sinks: aws_s3: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/about/under-the-hood/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type: object: options: enabled: {
			description: """
				Whether or not end-to-end acknowledgements are enabled.

				When enabled for a sink, any source that supports end-to-end
				acknowledgements that is connected to that sink waits for events
				to be acknowledged by **all connected sinks** before acknowledging them at the source.

				Enabling or disabling acknowledgements at the sink level takes precedence over any global
				[`acknowledgements`][global_acks] configuration.

				[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
				"""
			required: false
			type: bool: {}
		}
	}
	acl: {
		description: """
			Canned ACL to apply to the created objects.

			For more information, see [Canned ACL][canned_acl].

			[canned_acl]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#canned-acl
			"""
		required: false
		type: string: enum: {
			"authenticated-read": """
				Bucket/object can be read by authenticated users.

				The bucket/object owner is granted the `FULL_CONTROL` permission, and anyone in the
				`AuthenticatedUsers` grantee group is granted the `READ` permission.
				"""
			"aws-exec-read": """
				Bucket/object are private, and readable by EC2.

				The bucket/object owner is granted the `FULL_CONTROL` permission, and the AWS EC2 service is
				granted the `READ` permission for the purpose of reading Amazon Machine Image (AMI) bundles
				from the given bucket.
				"""
			"bucket-owner-full-control": """
				Object is semi-private.

				Both the object owner and bucket owner are granted the `FULL_CONTROL` permission.

				Only relevant when specified for an object: this canned ACL is otherwise ignored when
				specified for a bucket.
				"""
			"bucket-owner-read": """
				Object is private, except to the bucket owner.

				The object owner is granted the `FULL_CONTROL` permission, and the bucket owner is granted the `READ` permission.

				Only relevant when specified for an object: this canned ACL is otherwise ignored when
				specified for a bucket.
				"""
			"log-delivery-write": """
				Bucket can have logs written.

				The `LogDelivery` grantee group is granted `WRITE` and `READ_ACP` permissions.

				Only relevant when specified for a bucket: this canned ACL is otherwise ignored when
				specified for an object.

				For more information about logs, see [Amazon S3 Server Access Logging][serverlogs].

				[serverlogs]: https://docs.aws.amazon.com/AmazonS3/latest/dev/ServerLogs.html
				"""
			private: """
				Bucket/object are private.

				The bucket/object owner is granted the `FULL_CONTROL` permission, and no one else has
				access.

				This is the default.
				"""
			"public-read": """
				Bucket/object can be read publicly.

				The bucket/object owner is granted the `FULL_CONTROL` permission, and anyone in the
				`AllUsers` grantee group is granted the `READ` permission.
				"""
			"public-read-write": """
				Bucket/object can be read and written publicly.

				The bucket/object owner is granted the `FULL_CONTROL` permission, and anyone in the
				`AllUsers` grantee group is granted the `READ` and `WRITE` permissions.

				This is generally not recommended.
				"""
		}
	}
	auth: {
		description: "Configuration of the authentication strategy for interacting with AWS services."
		required:    false
		type: object: options: {
			access_key_id: {
				description: "The AWS access key ID."
				required:    true
				type: string: examples: ["AKIAIOSFODNN7EXAMPLE"]
			}
			assume_role: {
				description: """
					The ARN of an [IAM role][iam_role] to assume.

					[iam_role]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles.html
					"""
				required: true
				type: string: examples: ["arn:aws:iam::123456789098:role/my_role"]
			}
			credentials_file: {
				description: "Path to the credentials file."
				required:    true
				type: string: examples: ["/my/aws/credentials"]
			}
			external_id: {
				description: """
					The optional unique external ID in conjunction with role to assume.

					[external_id]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles_create_for-user_externalid.html
					"""
				required: false
				type: string: examples: ["randomEXAMPLEidString"]
			}
			imds: {
				description: "Configuration for authenticating with AWS through IMDS."
				required:    false
				type: object: options: {
					connect_timeout_seconds: {
						description: "Connect timeout for IMDS."
						required:    false
						type: uint: {
							default: 1
							unit:    "seconds"
						}
					}
					max_attempts: {
						description: "Number of IMDS retries for fetching tokens and metadata."
						required:    false
						type: uint: default: 4
					}
					read_timeout_seconds: {
						description: "Read timeout for IMDS."
						required:    false
						type: uint: {
							default: 1
							unit:    "seconds"
						}
					}
				}
			}
			load_timeout_secs: {
				description: """
					Timeout for successfully loading any credentials, in seconds.

					Relevant when the default credentials chain or `assume_role` is used.
					"""
				required: false
				type: uint: {
					examples: [30]
					unit: "seconds"
				}
			}
			profile: {
				description: """
					The credentials profile to use.

					Used to select AWS credentials from a provided credentials file.
					"""
				required: false
				type: string: {
					default: "default"
					examples: ["develop"]
				}
			}
			region: {
				description: """
					The [AWS region][aws_region] to send STS requests to.

					If not set, this defaults to the configured region
					for the service itself.

					[aws_region]: https://docs.aws.amazon.com/general/latest/gr/rande.html#regional-endpoints
					"""
				required: false
				type: string: examples: ["us-west-2"]
			}
			secret_access_key: {
				description: "The AWS secret access key."
				required:    true
				type: string: examples: ["wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"]
			}
			session_name: {
				description: """
					The optional [RoleSessionName][role_session_name] is a unique session identifier for your assumed role.

					Should be unique per principal or reason.
					If not set, session name will be autogenerated like assume-role-provider-1736428351340

					[role_session_name]: https://docs.aws.amazon.com/STS/latest/APIReference/API_AssumeRole.html
					"""
				required: false
				type: string: examples: ["vector-indexer-role"]
			}
		}
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type: object: options: {
			max_bytes: {
				description: """
					The maximum size of a batch that is processed by a sink.

					This is based on the uncompressed size of the batched events, before they are
					serialized/compressed.
					"""
				required: false
				type: uint: {
					default: 10000000
					unit:    "bytes"
				}
			}
			max_events: {
				description: "The maximum size of a batch before it is flushed."
				required:    false
				type: uint: unit: "events"
			}
			timeout_secs: {
				description: "The maximum age of a batch before it is flushed."
				required:    false
				type: float: {
					default: 300.0
					unit:    "seconds"
				}
			}
		}
	}
	bucket: {
		description: """
			The S3 bucket name.

			This must not include a leading `s3://` or a trailing `/`.
			"""
		required: true
		type: string: examples: ["my-bucket"]
	}
	compression: {
		description: """
			Compression configuration.

			All compression algorithms use the default compression level unless otherwise specified.

			Some cloud storage API clients and browsers handle decompression transparently, so
			depending on how they are accessed, files may not always appear to be compressed.
			"""
		required: false
		type: string: {
			default: "gzip"
			enum: {
				gzip: """
					[Gzip][gzip] compression.

					[gzip]: https://www.gzip.org/
					"""
				none: "No compression."
				snappy: """
					[Snappy][snappy] compression.

					[snappy]: https://github.com/google/snappy/blob/main/docs/README.md
					"""
				zlib: """
					[Zlib][zlib] compression.

					[zlib]: https://zlib.net/
					"""
				zstd: """
					[Zstandard][zstd] compression.

					[zstd]: https://facebook.github.io/zstd/
					"""
			}
		}
	}
	content_encoding: {
		description: """
			Overrides what content encoding has been applied to the object.

			Directly comparable to the `Content-Encoding` HTTP header.

			If not specified, the compression scheme used dictates this value.
			"""
		required: false
		type: string: examples: [
			"gzip",
		]
	}
	content_type: {
		description: """
			Overrides the MIME type of the object.

			Directly comparable to the `Content-Type` HTTP header.

			If not specified, the compression scheme used dictates this value.
			When `compression` is set to `none`, the value `text/x-log` is used.
			"""
		required: false
		type: string: examples: ["application/gzip"]
	}
	encoding: {
		description: "Configures how events are encoded into raw bytes."
		required:    true
		type: object: options: {
			avro: {
				description:   "Apache Avro-specific encoder options."
				relevant_when: "codec = \"avro\""
				required:      true
				type: object: options: schema: {
					description: "The Avro schema."
					required:    true
					type: string: examples: ["{ \"type\": \"record\", \"name\": \"log\", \"fields\": [{ \"name\": \"message\", \"type\": \"string\" }] }"]
				}
			}
			cef: {
				description:   "The CEF Serializer Options."
				relevant_when: "codec = \"cef\""
				required:      true
				type: object: options: {
					device_event_class_id: {
						description: """
																Unique identifier for each event type. Identifies the type of event reported.
																The value length must be less than or equal to 1023.
																"""
						required: true
						type: string: {}
					}
					device_product: {
						description: """
																Identifies the product of a vendor.
																The part of a unique device identifier. No two products can use the same combination of device vendor and device product.
																The value length must be less than or equal to 63.
																"""
						required: true
						type: string: {}
					}
					device_vendor: {
						description: """
																Identifies the vendor of the product.
																The part of a unique device identifier. No two products can use the same combination of device vendor and device product.
																The value length must be less than or equal to 63.
																"""
						required: true
						type: string: {}
					}
					device_version: {
						description: """
																Identifies the version of the problem. The combination of the device product, vendor and this value make up the unique id of the device that sends messages.
																The value length must be less than or equal to 31.
																"""
						required: true
						type: string: {}
					}
					extensions: {
						description: """
																The collection of key-value pairs. Keys are the keys of the extensions, and values are paths that point to the extension values of a log event.
																The event can have any number of key-value pairs in any order.
																"""
						required: false
						type: object: options: "*": {
							description: "This is a path that points to the extension value of a log event."
							required:    true
							type: string: {}
						}
					}
					name: {
						description: """
																This is a path that points to the human-readable description of a log event.
																The value length must be less than or equal to 512.
																Equals "cef.name" by default.
																"""
						required: true
						type: string: {}
					}
					severity: {
						description: """
																This is a path that points to the field of a log event that reflects importance of the event.
																Reflects importance of the event.

																It must point to a number from 0 to 10.
																0 = lowest_importance, 10 = highest_importance.
																Set to "cef.severity" by default.
																"""
						required: true
						type: string: {}
					}
					version: {
						description: """
																CEF Version. Can be either 0 or 1.
																Set to "0" by default.
																"""
						required: true
						type: string: enum: {
							V0: "CEF specification version 0.1."
							V1: "CEF specification version 1.x."
						}
					}
				}
			}
			codec: {
				description: "The codec to use for encoding events."
				required:    true
				type: string: enum: {
					avro: """
						Encodes an event as an [Apache Avro][apache_avro] message.

						[apache_avro]: https://avro.apache.org/
						"""
					cef: "Encodes an event as a CEF (Common Event Format) formatted message."
					csv: """
						Encodes an event as a CSV message.

						This codec must be configured with fields to encode.
						"""
					gelf: """
						Encodes an event as a [GELF][gelf] message.

						This codec is experimental for the following reason:

						The GELF specification is more strict than the actual Graylog receiver.
						Vector's encoder currently adheres more strictly to the GELF spec, with
						the exception that some characters such as `@`  are allowed in field names.

						Other GELF codecs, such as Loki's, use a [Go SDK][implementation] that is maintained
						by Graylog and is much more relaxed than the GELF spec.

						Going forward, Vector will use that [Go SDK][implementation] as the reference implementation, which means
						the codec might continue to relax the enforcement of the specification.

						[gelf]: https://docs.graylog.org/docs/gelf
						[implementation]: https://github.com/Graylog2/go-gelf/blob/v2/gelf/reader.go
						"""
					json: """
						Encodes an event as [JSON][json].

						[json]: https://www.json.org/
						"""
					logfmt: """
						Encodes an event as a [logfmt][logfmt] message.

						[logfmt]: https://brandur.org/logfmt
						"""
					native: """
						Encodes an event in the [native Protocol Buffers format][vector_native_protobuf].

						This codec is **[experimental][experimental]**.

						[vector_native_protobuf]: https://github.com/vectordotdev/vector/blob/master/lib/vector-core/proto/event.proto
						[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
						"""
					native_json: """
						Encodes an event in the [native JSON format][vector_native_json].

						This codec is **[experimental][experimental]**.

						[vector_native_json]: https://github.com/vectordotdev/vector/blob/master/lib/codecs/tests/data/native_encoding/schema.cue
						[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
						"""
					protobuf: """
						Encodes an event as a [Protobuf][protobuf] message.

						[protobuf]: https://protobuf.dev/
						"""
					raw_message: """
						No encoding.

						This encoding uses the `message` field of a log event.

						Be careful if you are modifying your log events (for example, by using a `remap`
						transform) and removing the message field while doing additional parsing on it, as this
						could lead to the encoding emitting empty strings for the given event.
						"""
					text: """
						Plain text encoding.

						This encoding uses the `message` field of a log event. For metrics, it uses an
						encoding that resembles the Prometheus export format.

						Be careful if you are modifying your log events (for example, by using a `remap`
						transform) and removing the message field while doing additional parsing on it, as this
						could lead to the encoding emitting empty strings for the given event.
						"""
				}
			}
			csv: {
				description:   "The CSV Serializer Options."
				relevant_when: "codec = \"csv\""
				required:      true
				type: object: options: {
					capacity: {
						description: """
																Sets the capacity (in bytes) of the internal buffer used in the CSV writer.
																This defaults to 8KB.
																"""
						required: false
						type: uint: default: 8192
					}
					delimiter: {
						description: "The field delimiter to use when writing CSV."
						required:    false
						type: ascii_char: default: ","
					}
					double_quote: {
						description: """
																Enables double quote escapes.

																This is enabled by default, but you can disable it. When disabled, quotes in
																field data are escaped instead of doubled.
																"""
						required: false
						type: bool: default: true
					}
					escape: {
						description: """
																The escape character to use when writing CSV.

																In some variants of CSV, quotes are escaped using a special escape character
																like \\ (instead of escaping quotes by doubling them).

																To use this, `double_quotes` needs to be disabled as well; otherwise, this setting is ignored.
																"""
						required: false
						type: ascii_char: default: "\""
					}
					fields: {
						description: """
																Configures the fields that are encoded, as well as the order in which they
																appear in the output.

																If a field is not present in the event, the output for that field is an empty string.

																Values of type `Array`, `Object`, and `Regex` are not supported, and the
																output for any of these types is an empty string.
																"""
						required: true
						type: array: items: type: string: {}
					}
					quote: {
						description: "The quote character to use when writing CSV."
						required:    false
						type: ascii_char: default: "\""
					}
					quote_style: {
						description: "The quoting style to use when writing CSV data."
						required:    false
						type: string: {
							default: "necessary"
							enum: {
								always: "Always puts quotes around every field."
								necessary: """
																			Puts quotes around fields only when necessary.
																			They are necessary when fields contain a quote, delimiter, or record terminator.
																			Quotes are also necessary when writing an empty record
																			(which is indistinguishable from a record with one empty field).
																			"""
								never: "Never writes quotes, even if it produces invalid CSV data."
								non_numeric: """
																			Puts quotes around all fields that are non-numeric.
																			This means that when writing a field that does not parse as a valid float or integer,
																			quotes are used even if they aren't strictly necessary.
																			"""
							}
						}
					}
				}
			}
			except_fields: {
				description: "List of fields that are excluded from the encoded event."
				required:    false
				type: array: items: type: string: {}
			}
			json: {
				description:   "Options for the JsonSerializer."
				relevant_when: "codec = \"json\""
				required:      false
				type: object: options: pretty: {
					description: "Whether to use pretty JSON formatting."
					required:    false
					type: bool: default: false
				}
			}
			metric_tag_values: {
				description: """
					Controls how metric tag values are encoded.

					When set to `single`, only the last non-bare value of tags are displayed with the
					metric.  When set to `full`, all metric tags are exposed as separate assignments.
					"""
				relevant_when: "codec = \"json\" or codec = \"text\""
				required:      false
				type: string: {
					default: "single"
					enum: {
						full: "All tags are exposed as arrays of either string or null values."
						single: """
															Tag values are exposed as single strings, the same as they were before this config
															option. Tags with multiple values show the last assigned value, and null values
															are ignored.
															"""
					}
				}
			}
			only_fields: {
				description: "List of fields that are included in the encoded event."
				required:    false
				type: array: items: type: string: {}
			}
			protobuf: {
				description:   "Options for the Protobuf serializer."
				relevant_when: "codec = \"protobuf\""
				required:      true
				type: object: options: {
					desc_file: {
						description: """
																The path to the protobuf descriptor set file.

																This file is the output of `protoc -I <include path> -o <desc output path> <proto>`

																You can read more [here](https://buf.build/docs/reference/images/#how-buf-images-work).
																"""
						required: true
						type: string: examples: ["/etc/vector/protobuf_descriptor_set.desc"]
					}
					message_type: {
						description: "The name of the message type to use for serializing."
						required:    true
						type: string: examples: ["package.Message"]
					}
				}
			}
			timestamp_format: {
				description: "Format used for timestamp fields."
				required:    false
				type: string: enum: {
					rfc3339:    "Represent the timestamp as a RFC 3339 timestamp."
					unix:       "Represent the timestamp as a Unix timestamp."
					unix_float: "Represent the timestamp as a Unix timestamp in floating point."
					unix_ms:    "Represent the timestamp as a Unix timestamp in milliseconds."
					unix_ns:    "Represent the timestamp as a Unix timestamp in nanoseconds."
					unix_us:    "Represent the timestamp as a Unix timestamp in microseconds"
				}
			}
		}
	}
	endpoint: {
		description: "Custom endpoint for use with AWS-compatible services."
		required:    false
		type: string: examples: ["http://127.0.0.0:5000/path/to/service"]
	}
	filename_append_uuid: {
		description: """
			Whether or not to append a UUID v4 token to the end of the object key.

			The UUID is appended to the timestamp portion of the object key, such that if the object key
			generated is `date=2022-07-18/1658176486`, setting this field to `true` results
			in an object key that looks like `date=2022-07-18/1658176486-30f6652c-71da-4f9f-800d-a1189c47c547`.

			This ensures there are no name collisions, and can be useful in high-volume workloads where
			object keys must be unique.
			"""
		required: false
		type: bool: default: true
	}
	filename_extension: {
		description: """
			The filename extension to use in the object key.

			This overrides setting the extension based on the configured `compression`.
			"""
		required: false
		type: string: examples: [
			"json",
		]
	}
	filename_time_format: {
		description: """
			The timestamp format for the time component of the object key.

			By default, object keys are appended with a timestamp that reflects when the objects are
			sent to S3, such that the resulting object key is functionally equivalent to joining the key
			prefix with the formatted timestamp, such as `date=2022-07-18/1658176486`.

			This would represent a `key_prefix` set to `date=%F/` and the timestamp of Mon Jul 18 2022
			20:34:44 GMT+0000, with the `filename_time_format` being set to `%s`, which renders
			timestamps in seconds since the Unix epoch.

			Supports the common [`strftime`][chrono_strftime_specifiers] specifiers found in most
			languages.

			When set to an empty string, no timestamp is appended to the key prefix.

			[chrono_strftime_specifiers]: https://docs.rs/chrono/latest/chrono/format/strftime/index.html#specifiers
			"""
		required: false
		type: string: default: "%s"
	}
	force_path_style: {
		description: """
			Specifies which addressing style to use.

			This controls if the bucket name is in the hostname or part of the URL.
			"""
		required: false
		type: bool: default: true
	}
	framing: {
		description: "Framing configuration."
		required:    false
		type: object: options: {
			character_delimited: {
				description:   "Options for the character delimited encoder."
				relevant_when: "method = \"character_delimited\""
				required:      true
				type: object: options: delimiter: {
					description: "The ASCII (7-bit) character that delimits byte sequences."
					required:    true
					type: ascii_char: {}
				}
			}
			length_delimited: {
				description:   "Options for the length delimited decoder."
				relevant_when: "method = \"length_delimited\""
				required:      true
				type: object: options: {
					length_field_is_big_endian: {
						description: "Length field byte order (little or big endian)"
						required:    false
						type: bool: default: true
					}
					length_field_length: {
						description: "Number of bytes representing the field length"
						required:    false
						type: uint: default: 4
					}
					length_field_offset: {
						description: "Number of bytes in the header before the length field"
						required:    false
						type: uint: default: 0
					}
					max_frame_length: {
						description: "Maximum frame length"
						required:    false
						type: uint: default: 8388608
					}
				}
			}
			method: {
				description: "The framing method."
				required:    true
				type: string: enum: {
					bytes:               "Event data is not delimited at all."
					character_delimited: "Event data is delimited by a single ASCII (7-bit) character."
					length_delimited: """
						Event data is prefixed with its length in bytes.

						The prefix is a 32-bit unsigned integer, little endian.
						"""
					newline_delimited: "Event data is delimited by a newline (LF) character."
				}
			}
		}
	}
	grant_full_control: {
		description: """
			Grants `READ`, `READ_ACP`, and `WRITE_ACP` permissions on the created objects to the named [grantee].

			This allows the grantee to read the created objects and their metadata, as well as read and
			modify the ACL on the created objects.

			[grantee]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#specifying-grantee
			"""
		required: false
		type: string: examples: ["79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be", "person@email.com", "http://acs.amazonaws.com/groups/global/AllUsers"]
	}
	grant_read: {
		description: """
			Grants `READ` permissions on the created objects to the named [grantee].

			This allows the grantee to read the created objects and their metadata.

			[grantee]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#specifying-grantee
			"""
		required: false
		type: string: examples: ["79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be", "person@email.com", "http://acs.amazonaws.com/groups/global/AllUsers"]
	}
	grant_read_acp: {
		description: """
			Grants `READ_ACP` permissions on the created objects to the named [grantee].

			This allows the grantee to read the ACL on the created objects.

			[grantee]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#specifying-grantee
			"""
		required: false
		type: string: examples: ["79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be", "person@email.com", "http://acs.amazonaws.com/groups/global/AllUsers"]
	}
	grant_write_acp: {
		description: """
			Grants `WRITE_ACP` permissions on the created objects to the named [grantee].

			This allows the grantee to modify the ACL on the created objects.

			[grantee]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#specifying-grantee
			"""
		required: false
		type: string: examples: ["79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be", "person@email.com", "http://acs.amazonaws.com/groups/global/AllUsers"]
	}
	key_prefix: {
		description: """
			A prefix to apply to all object keys.

			Prefixes are useful for partitioning objects, such as by creating an object key that
			stores objects under a particular directory. If using a prefix for this purpose, it must end
			in `/` to act as a directory path. A trailing `/` is **not** automatically added.
			"""
		required: false
		type: string: {
			default: "date=%F"
			examples: ["date=%F/hour=%H", "year=%Y/month=%m/day=%d", "application_id={{ application_id }}/date=%F"]
			syntax: "template"
		}
	}
	region: {
		description: """
			The [AWS region][aws_region] of the target service.

			[aws_region]: https://docs.aws.amazon.com/general/latest/gr/rande.html#regional-endpoints
			"""
		required: false
		type: string: examples: ["us-east-1"]
	}
	request: {
		description: """
			Middleware settings for outbound requests.

			Various settings can be configured, such as concurrency and rate limits, timeouts, and retry behavior.

			Note that the retry backoff policy follows the Fibonacci sequence.
			"""
		required: false
		type: object: options: {
			adaptive_concurrency: {
				description: """
					Configuration of adaptive concurrency parameters.

					These parameters typically do not require changes from the default, and incorrect values can lead to meta-stable or
					unstable performance and sink behavior. Proceed with caution.
					"""
				required: false
				type: object: options: {
					decrease_ratio: {
						description: """
																The fraction of the current value to set the new concurrency limit when decreasing the limit.

																Valid values are greater than `0` and less than `1`. Smaller values cause the algorithm to scale back rapidly
																when latency increases.

																**Note**: The new limit is rounded down after applying this ratio.
																"""
						required: false
						type: float: default: 0.9
					}
					ewma_alpha: {
						description: """
																The weighting of new measurements compared to older measurements.

																Valid values are greater than `0` and less than `1`.

																ARC uses an exponentially weighted moving average (EWMA) of past RTT measurements as a reference to compare with
																the current RTT. Smaller values cause this reference to adjust more slowly, which may be useful if a service has
																unusually high response variability.
																"""
						required: false
						type: float: default: 0.4
					}
					initial_concurrency: {
						description: """
																The initial concurrency limit to use. If not specified, the initial limit is 1 (no concurrency).

																Datadog recommends setting this value to your service's average limit if you're seeing that it takes a
																long time to ramp up adaptive concurrency after a restart. You can find this value by looking at the
																`adaptive_concurrency_limit` metric.
																"""
						required: false
						type: uint: default: 1
					}
					max_concurrency_limit: {
						description: """
																The maximum concurrency limit.

																The adaptive request concurrency limit does not go above this bound. This is put in place as a safeguard.
																"""
						required: false
						type: uint: default: 200
					}
					rtt_deviation_scale: {
						description: """
																Scale of RTT deviations which are not considered anomalous.

																Valid values are greater than or equal to `0`, and we expect reasonable values to range from `1.0` to `3.0`.

																When calculating the past RTT average, we also compute a secondary “deviation” value that indicates how variable
																those values are. We use that deviation when comparing the past RTT average to the current measurements, so we
																can ignore increases in RTT that are within an expected range. This factor is used to scale up the deviation to
																an appropriate range.  Larger values cause the algorithm to ignore larger increases in the RTT.
																"""
						required: false
						type: float: default: 2.5
					}
				}
			}
			concurrency: {
				description: """
					Configuration for outbound request concurrency.

					This can be set either to one of the below enum values or to a positive integer, which denotes
					a fixed concurrency limit.
					"""
				required: false
				type: {
					string: {
						default: "adaptive"
						enum: {
							adaptive: """
															Concurrency is managed by Vector's [Adaptive Request Concurrency][arc] feature.

															[arc]: https://vector.dev/docs/about/under-the-hood/networking/arc/
															"""
							none: """
															A fixed concurrency of 1.

															Only one request can be outstanding at any given time.
															"""
						}
					}
					uint: {}
				}
			}
			rate_limit_duration_secs: {
				description: "The time window used for the `rate_limit_num` option."
				required:    false
				type: uint: {
					default: 1
					unit:    "seconds"
				}
			}
			rate_limit_num: {
				description: "The maximum number of requests allowed within the `rate_limit_duration_secs` time window."
				required:    false
				type: uint: {
					default: 9223372036854775807
					unit:    "requests"
				}
			}
			retry_attempts: {
				description: "The maximum number of retries to make for failed requests."
				required:    false
				type: uint: {
					default: 9223372036854775807
					unit:    "retries"
				}
			}
			retry_initial_backoff_secs: {
				description: """
					The amount of time to wait before attempting the first retry for a failed request.

					After the first retry has failed, the fibonacci sequence is used to select future backoffs.
					"""
				required: false
				type: uint: {
					default: 1
					unit:    "seconds"
				}
			}
			retry_jitter_mode: {
				description: "The jitter mode to use for retry backoff behavior."
				required:    false
				type: string: {
					default: "Full"
					enum: {
						Full: """
															Full jitter.

															The random delay is anywhere from 0 up to the maximum current delay calculated by the backoff
															strategy.

															Incorporating full jitter into your backoff strategy can greatly reduce the likelihood
															of creating accidental denial of service (DoS) conditions against your own systems when
															many clients are recovering from a failure state.
															"""
						None: "No jitter."
					}
				}
			}
			retry_max_duration_secs: {
				description: "The maximum amount of time to wait between retries."
				required:    false
				type: uint: {
					default: 30
					unit:    "seconds"
				}
			}
			timeout_secs: {
				description: """
					The time a request can take before being aborted.

					Datadog highly recommends that you do not lower this value below the service's internal timeout, as this could
					create orphaned requests, pile on retries, and result in duplicate data downstream.
					"""
				required: false
				type: uint: {
					default: 60
					unit:    "seconds"
				}
			}
		}
	}
	server_side_encryption: {
		description: """
			AWS S3 Server-Side Encryption algorithms.

			The Server-side Encryption algorithm used when storing these objects.
			"""
		required: false
		type: string: enum: {
			AES256: """
				Each object is encrypted with AES-256 using a unique key.

				This corresponds to the `SSE-S3` option.
				"""
			"aws:kms": """
				Each object is encrypted with AES-256 using keys managed by AWS KMS.

				Depending on whether or not a KMS key ID is specified, this corresponds either to the
				`SSE-KMS` option (keys generated/managed by KMS) or the `SSE-C` option (keys generated by
				the customer, managed by KMS).
				"""
		}
	}
	ssekms_key_id: {
		description: """
			Specifies the ID of the AWS Key Management Service (AWS KMS) symmetrical customer managed
			customer master key (CMK) that is used for the created objects.

			Only applies when `server_side_encryption` is configured to use KMS.

			If not specified, Amazon S3 uses the AWS managed CMK in AWS to protect the data.
			"""
		required: false
		type: string: {
			examples: ["abcd1234"]
			syntax: "template"
		}
	}
	storage_class: {
		description: """
			The storage class for the created objects.

			See the [S3 Storage Classes][s3_storage_classes] for more details.

			[s3_storage_classes]: https://docs.aws.amazon.com/AmazonS3/latest/dev/storage-class-intro.html
			"""
		required: false
		type: string: {
			default: "STANDARD"
			enum: {
				DEEP_ARCHIVE:        "Glacier Deep Archive."
				EXPRESS_ONEZONE:     "High Performance (single Availability zone)."
				GLACIER:             "Glacier Flexible Retrieval."
				GLACIER_IR:          "Glacier Instant Retrieval."
				INTELLIGENT_TIERING: "Intelligent Tiering."
				ONEZONE_IA:          "Infrequently Accessed (single Availability zone)."
				REDUCED_REDUNDANCY:  "Reduced Redundancy."
				STANDARD:            "Standard Redundancy."
				STANDARD_IA:         "Infrequently Accessed."
			}
		}
	}
	tags: {
		description: "The tag-set for the object."
		required:    false
		type: object: {
			examples: [{
				Classification: "confidential"
				PHI:            "True"
				Project:        "Blue"
			}]
			options: "*": {
				description: "A single tag."
				required:    true
				type: string: {}
			}
		}
	}
	timezone: {
		description: """
			Timezone to use for any date specifiers in template strings.

			This can refer to any valid timezone as defined in the [TZ database][tzdb], or "local" which refers to the system local timezone. It will default to the [globally configured timezone](https://vector.dev/docs/reference/configuration/global-options/#timezone).

			[tzdb]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
			"""
		required: false
		type: string: examples: ["local", "America/New_York", "EST5EDT"]
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type: object: options: {
			alpn_protocols: {
				description: """
					Sets the list of supported ALPN protocols.

					Declare the supported ALPN protocols, which are used during negotiation with a peer. They are prioritized in the order
					that they are defined.
					"""
				required: false
				type: array: items: type: string: examples: ["h2"]
			}
			ca_file: {
				description: """
					Absolute path to an additional CA certificate file.

					The certificate must be in the DER or PEM (X.509) format. Additionally, the certificate can be provided as an inline string in PEM format.
					"""
				required: false
				type: string: examples: ["/path/to/certificate_authority.crt"]
			}
			crt_file: {
				description: """
					Absolute path to a certificate file used to identify this server.

					The certificate must be in DER, PEM (X.509), or PKCS#12 format. Additionally, the certificate can be provided as
					an inline string in PEM format.

					If this is set _and_ is not a PKCS#12 archive, `key_file` must also be set.
					"""
				required: false
				type: string: examples: ["/path/to/host_certificate.crt"]
			}
			key_file: {
				description: """
					Absolute path to a private key file used to identify this server.

					The key must be in DER or PEM (PKCS#8) format. Additionally, the key can be provided as an inline string in PEM format.
					"""
				required: false
				type: string: examples: ["/path/to/host_certificate.key"]
			}
			key_pass: {
				description: """
					Passphrase used to unlock the encrypted key file.

					This has no effect unless `key_file` is set.
					"""
				required: false
				type: string: examples: ["${KEY_PASS_ENV_VAR}", "PassWord1"]
			}
			server_name: {
				description: """
					Server name to use when using Server Name Indication (SNI).

					Only relevant for outgoing connections.
					"""
				required: false
				type: string: examples: ["www.example.com"]
			}
			verify_certificate: {
				description: """
					Enables certificate verification. For components that create a server, this requires that the
					client connections have a valid client certificate. For components that initiate requests,
					this validates that the upstream has a valid certificate.

					If enabled, certificates must not be expired and must be issued by a trusted
					issuer. This verification operates in a hierarchical manner, checking that the leaf certificate (the
					certificate presented by the client/server) is not only valid, but that the issuer of that certificate is also valid, and
					so on, until the verification process reaches a root certificate.

					Do NOT set this to `false` unless you understand the risks of not verifying the validity of certificates.
					"""
				required: false
				type: bool: {}
			}
			verify_hostname: {
				description: """
					Enables hostname verification.

					If enabled, the hostname used to connect to the remote host must be present in the TLS certificate presented by
					the remote host, either as the Common Name or as an entry in the Subject Alternative Name extension.

					Only relevant for outgoing connections.

					Do NOT set this to `false` unless you understand the risks of not verifying the remote hostname.
					"""
				required: false
				type: bool: {}
			}
		}
	}
}
