package metadata

generated: components: sources: azure_blob: configuration: {
	account_name: {
		description: """
			The Azure Blob Storage Account name.

			If provided, this is used instead of the `connection_string` and requires `auth` to be
			configured. Both the blob and queue service endpoints are derived from the account name.
			"""
		required: false
		type: string: examples: ["mylogstorage"]
	}
	acknowledgements: {
		deprecated: true
		description: """
			Controls how acknowledgements are handled by this source.

			This setting is **deprecated** in favor of enabling `acknowledgements` at the [global][global_acks] or sink level.

			Enabling or disabling acknowledgements at the source level has **no effect** on acknowledgement behavior.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type: object: options: enabled: {
			description: "Whether or not end-to-end acknowledgements are enabled for this source."
			required:    false
			type: bool: {}
		}
	}
	auth: {
		description: "Azure service principal authentication."
		required:    false
		type: object: options: {
			azure_client_id: {
				description: """
					The [Azure Client ID][azure_client_id].

					[azure_client_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
					"""
				relevant_when: "azure_credential_kind = \"client_certificate_credential\" or azure_credential_kind = \"client_secret_credential\""
				required:      true
				type: string: examples: ["00000000-0000-0000-0000-000000000000", "${AZURE_CLIENT_ID:?err}"]
			}
			azure_client_secret: {
				description: """
					The [Azure Client Secret][azure_client_secret].

					[azure_client_secret]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
					"""
				relevant_when: "azure_credential_kind = \"client_secret_credential\""
				required:      true
				type: string: examples: ["00-00~000000-0000000~0000000000000000000", "${AZURE_CLIENT_SECRET:?err}"]
			}
			azure_credential_kind: {
				description: "The kind of Azure credential to use."
				required:    true
				type: string: enum: {
					azure_cli:                         "Use Azure CLI credentials"
					client_certificate_credential:     "Use certificate credentials"
					client_secret_credential:          "Use client ID/secret credentials"
					managed_identity:                  "Use Managed Identity credentials"
					managed_identity_client_assertion: "Use Managed Identity with Client Assertion credentials"
					workload_identity:                 "Use Workload Identity credentials"
				}
			}
			azure_tenant_id: {
				description: """
					The [Azure Tenant ID][azure_tenant_id].

					[azure_tenant_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
					"""
				relevant_when: "azure_credential_kind = \"client_certificate_credential\" or azure_credential_kind = \"client_secret_credential\""
				required:      true
				type: string: examples: ["00000000-0000-0000-0000-000000000000", "${AZURE_TENANT_ID:?err}"]
			}
			certificate_password: {
				description:   "The password for the client certificate, if applicable."
				relevant_when: "azure_credential_kind = \"client_certificate_credential\""
				required:      false
				type: string: examples: ["${AZURE_CLIENT_CERTIFICATE_PASSWORD}"]
			}
			certificate_path: {
				description:   "PKCS12 certificate with RSA private key."
				relevant_when: "azure_credential_kind = \"client_certificate_credential\""
				required:      true
				type: string: examples: ["path/to/certificate.pfx", "${AZURE_CLIENT_CERTIFICATE_PATH:?err}"]
			}
			client_assertion_client_id: {
				description:   "The target Client ID to use."
				relevant_when: "azure_credential_kind = \"managed_identity_client_assertion\""
				required:      true
				type: string: examples: ["00000000-0000-0000-0000-000000000000"]
			}
			client_assertion_tenant_id: {
				description:   "The target Tenant ID to use."
				relevant_when: "azure_credential_kind = \"managed_identity_client_assertion\""
				required:      true
				type: string: examples: ["00000000-0000-0000-0000-000000000000"]
			}
			client_id: {
				description: """
					The [Azure Client ID][azure_client_id]. Defaults to the value of the environment variable `AZURE_CLIENT_ID`.

					[azure_client_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
					"""
				relevant_when: "azure_credential_kind = \"workload_identity\""
				required:      false
				type: string: examples: ["00000000-0000-0000-0000-000000000000", "${AZURE_CLIENT_ID}"]
			}
			tenant_id: {
				description: """
					The [Azure Tenant ID][azure_tenant_id]. Defaults to the value of the environment variable `AZURE_TENANT_ID`.

					[azure_tenant_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
					"""
				relevant_when: "azure_credential_kind = \"workload_identity\""
				required:      false
				type: string: examples: ["00000000-0000-0000-0000-000000000000", "${AZURE_TENANT_ID}"]
			}
			token_file_path: {
				description:   "Path of a file containing a Kubernetes service account token. Defaults to the value of the environment variable `AZURE_FEDERATED_TOKEN_FILE`."
				relevant_when: "azure_credential_kind = \"workload_identity\""
				required:      false
				type: string: examples: ["/var/run/secrets/azure/tokens/azure-identity-token", "${AZURE_FEDERATED_TOKEN_FILE}"]
			}
			user_assigned_managed_identity_id: {
				description:   "The User Assigned Managed Identity to use."
				relevant_when: "azure_credential_kind = \"managed_identity\" or azure_credential_kind = \"managed_identity_client_assertion\""
				required:      false
				type: string: examples: ["00000000-0000-0000-0000-000000000000"]
			}
			user_assigned_managed_identity_id_type: {
				description: """
					The type of the User Assigned Managed Identity ID provided (Client ID, Object ID,
					or Resource ID). Defaults to Client ID.
					"""
				relevant_when: "azure_credential_kind = \"managed_identity\" or azure_credential_kind = \"managed_identity_client_assertion\""
				required:      false
				type: string: enum: {
					client_id:   "Client ID"
					object_id:   "Object ID"
					resource_id: "Resource ID"
				}
			}
		}
	}
	blob_endpoint: {
		description: """
			The Azure Blob Storage service endpoint.

			Useful for Azurite, sovereign clouds, or private endpoints. Requires `auth` to be
			configured, and `queue_endpoint` to be provided as well when `account_name` is not set.
			"""
		required: false
		type: string: examples: ["https://mylogstorage.blob.core.windows.net/"]
	}
	compression: {
		description: "The compression scheme used for decompressing blobs retrieved from Azure Blob Storage."
		required:    false
		type: string: {
			default: "auto"
			enum: {
				auto: """
					Automatically attempt to determine the compression scheme.

					The compression scheme of the blob is determined from its `Content-Encoding` and
					`Content-Type` metadata, as well as the blob name suffix (for example, `.gz`).

					It is set to `none` if the compression scheme cannot be determined.
					"""
				gzip: "GZIP."
				none: "Uncompressed."
				zstd: "ZSTD."
			}
		}
	}
	connection_string: {
		description: """
			The Azure Blob Storage Account connection string.

			Authentication with an access key or shared access signature (SAS) are supported
			authentication methods. The connection string is also used to derive the blob and
			queue service endpoints.
			"""
		required: false
		type: string: examples: ["DefaultEndpointsProtocol=https;AccountName=mylogstorage;AccountKey=storageaccountkeybase64encoded;EndpointSuffix=core.windows.net", "BlobEndpoint=https://mylogstorage.blob.core.windows.net/;QueueEndpoint=https://mylogstorage.queue.core.windows.net/;SharedAccessSignature=generatedsastoken", "AccountName=mylogstorage"]
		warnings: ["Access keys and SAS tokens can be used to gain unauthorized access to Azure Storage resources. Numerous security breaches have occurred due to leaked connection strings. It is important to keep connection strings secure and not expose them in logs, error messages, or version control systems."]
	}
	decoding: {
		description: """
			Configures how events are decoded from raw bytes. Note some decoders can also determine the event output
			type (log, metric, trace).
			"""
		required: false
		type: object: options: {
			avro: {
				description:   "Apache Avro-specific encoder options."
				relevant_when: "codec = \"avro\""
				required:      true
				type: object: options: {
					schema: {
						description: """
																The Avro schema definition.
																**Note**: The following [`apache_avro::types::Value`] variants are *not* supported:
																* `Date`
																* `Decimal`
																* `Duration`
																* `Fixed`
																* `TimeMillis`
																"""
						required: true
						type: string: examples: ["{ \"type\": \"record\", \"name\": \"log\", \"fields\": [{ \"name\": \"message\", \"type\": \"string\" }] }"]
					}
					strip_schema_id_prefix: {
						description: "For Avro datum encoded in Kafka messages, the bytes are prefixed with the schema ID.  Set this to `true` to strip the schema ID prefix, as described in [Confluent Kafka's documentation](https://docs.confluent.io/platform/current/schema-registry/fundamentals/serdes-develop/index.html#wire-format)."
						required:    true
						type: bool: {}
					}
				}
			}
			codec: {
				description: "The codec to use for decoding events."
				required:    false
				type: string: {
					default: "bytes"
					enum: {
						avro: """
															Decodes the raw bytes as an [Apache Avro][apache_avro] message.

															[apache_avro]: https://avro.apache.org/
															"""
						bytes: "Uses the raw bytes as-is."
						gelf: """
															Decodes the raw bytes as a [GELF][gelf] message.

															This codec is experimental for the following reason:

															The GELF specification is more strict than the actual Graylog receiver.
															Vector's decoder adheres more strictly to the GELF spec, with
															the exception that some characters such as `@` are allowed in field names.

															Other GELF codecs, such as Loki's, use a [Go SDK][implementation] that is maintained
															by Graylog and is much more relaxed than the GELF spec.

															Going forward, Vector will use the [Go SDK][implementation] as the reference implementation, which means
															the codec may continue to relax the enforcement of the specification.

															[gelf]: https://docs.graylog.org/docs/gelf
															[implementation]: https://github.com/Graylog2/go-gelf/blob/v2/gelf/reader.go
															"""
						influxdb: """
															Decodes the raw bytes as an [Influxdb Line Protocol][influxdb] message.

															[influxdb]: https://docs.influxdata.com/influxdb/cloud/reference/syntax/line-protocol
															"""
						json: """
															Decodes the raw bytes as [JSON][json].

															[json]: https://www.json.org/
															"""
						native: """
															Decodes the raw bytes as [native Protocol Buffers format][vector_native_protobuf].

															This decoder can output all types of events: logs, metrics, and traces.

															This codec is **[experimental][experimental]**.

															[vector_native_protobuf]: https://github.com/vectordotdev/vector/blob/master/lib/vector-core/proto/event.proto
															[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
															"""
						native_json: """
															Decodes the raw bytes as [native JSON format][vector_native_json].

															This decoder can output all types of events: logs, metrics, and traces.

															This codec is **[experimental][experimental]**.

															[vector_native_json]: https://github.com/vectordotdev/vector/blob/master/lib/codecs/tests/data/native_encoding/schema.cue
															[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
															"""
						otlp: """
															Decodes the raw bytes as [OTLP (OpenTelemetry Protocol)][otlp] protobuf format.

															This decoder handles the three OTLP signal types: logs, metrics, and traces.
															It automatically detects which type of OTLP message is being decoded.

															[otlp]: https://opentelemetry.io/docs/specs/otlp/
															"""
						protobuf: """
															Decodes the raw bytes as [protobuf][protobuf].

															[protobuf]: https://protobuf.dev/
															"""
						syslog: """
															Decodes the raw bytes as a Syslog message.

															Decodes either as the [RFC 3164][rfc3164]-style format ("old" style) or the
															[RFC 5424][rfc5424]-style format ("new" style, includes structured data).

															[rfc3164]: https://www.ietf.org/rfc/rfc3164.txt
															[rfc5424]: https://www.ietf.org/rfc/rfc5424.txt
															"""
						vrl: """
															Decodes the raw bytes as a string and passes them as input to a [VRL][vrl] program.

															[vrl]: https://vector.dev/docs/reference/vrl
															"""
					}
				}
			}
			gelf: {
				description:   "GELF-specific decoding options."
				relevant_when: "codec = \"gelf\""
				required:      false
				type: object: options: {
					lossy: {
						description: """
																Determines whether to replace invalid UTF-8 sequences instead of failing.

																When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

																[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
																"""
						required: false
						type: bool: default: true
					}
					validation: {
						description: "Configures the decoding validation mode."
						required:    false
						type: string: {
							default: "strict"
							enum: {
								relaxed: """
																			Uses more relaxed validation that skips strict GELF specification checks.

																			This mode does not treat specification violations as errors, allowing the decoder
																			to accept messages from sources that don't strictly follow the GELF spec.
																			"""
								strict: "Uses strict validation that closely follows the GELF spec."
							}
						}
					}
				}
			}
			influxdb: {
				description:   "Influxdb-specific decoding options."
				relevant_when: "codec = \"influxdb\""
				required:      false
				type: object: options: lossy: {
					description: """
						Determines whether to replace invalid UTF-8 sequences instead of failing.

						When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

						[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
						"""
					required: false
					type: bool: default: true
				}
			}
			json: {
				description:   "JSON-specific decoding options."
				relevant_when: "codec = \"json\""
				required:      false
				type: object: options: lossy: {
					description: """
						Determines whether to replace invalid UTF-8 sequences instead of failing.

						When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

						[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
						"""
					required: false
					type: bool: default: true
				}
			}
			native_json: {
				description:   "Vector's native JSON-specific decoding options."
				relevant_when: "codec = \"native_json\""
				required:      false
				type: object: options: lossy: {
					description: """
						Determines whether to replace invalid UTF-8 sequences instead of failing.

						When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

						[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
						"""
					required: false
					type: bool: default: true
				}
			}
			protobuf: {
				description:   "Protobuf-specific decoding options."
				relevant_when: "codec = \"protobuf\""
				required:      false
				type: object: options: {
					desc_file: {
						description: """
																The path to the protobuf descriptor set file.

																This file is the output of `protoc -I <include path> -o <desc output path> <proto>`.

																For more information, see [How Buf images work](https://buf.build/docs/reference/images/#how-buf-images-work).
																"""
						required: false
						type: string: default: ""
					}
					message_type: {
						description: "The name of the message type to use for serializing."
						required:    false
						type: string: {
							default: ""
							examples: ["package.Message"]
						}
					}
					use_json_names: {
						description: """
																Use JSON field names (camelCase) instead of protobuf field names (snake_case).

																When enabled, the deserializer will output fields using their JSON names as defined
																in the `.proto` file (for example, `jobDescription` instead of `job_description`).

																This is useful when working with data that needs to be converted to JSON or
																when interfacing with systems that use JSON naming conventions.
																"""
						required: false
						type: bool: default: false
					}
				}
			}
			signal_types: {
				description: """
					Signal types to attempt parsing, in priority order.

					The deserializer tries to parse signals in the specified order. This allows you to optimize
					performance when you know the expected signal types. For example, if you only receive
					traces, set this to `["traces"]` to avoid attempting to parse as logs or metrics first.

					If not specified, defaults to trying all types in this order: logs, metrics, traces.
					Duplicate signal types are automatically removed while preserving order.
					"""
				relevant_when: "codec = \"otlp\""
				required:      false
				type: array: {
					default: ["logs", "metrics", "traces"]
					items: type: string: enum: {
						logs:    "OTLP logs signal (ExportLogsServiceRequest)"
						metrics: "OTLP metrics signal (ExportMetricsServiceRequest)"
						traces:  "OTLP traces signal (ExportTraceServiceRequest)"
					}
				}
			}
			syslog: {
				description:   "Syslog-specific decoding options."
				relevant_when: "codec = \"syslog\""
				required:      false
				type: object: options: lossy: {
					description: """
						Determines whether to replace invalid UTF-8 sequences instead of failing.

						When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

						[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
						"""
					required: false
					type: bool: default: true
				}
			}
			vrl: {
				description:   "VRL-specific decoding options."
				relevant_when: "codec = \"vrl\""
				required:      true
				type: object: options: {
					source: {
						description: """
																The [Vector Remap Language][vrl] (VRL) program to execute for each event.
																The final contents of the `.` target are used as the decoding result.
																Compilation errors or use of `abort` in the program result in a decoding error.

																[vrl]: https://vector.dev/docs/reference/vrl
																"""
						required: true
						type: string: {}
					}
					timezone: {
						description: """
																The name of the timezone to apply to timestamp conversions that do not contain an explicit
																time zone. The time zone name may be any name in the [TZ database][tz_database], or `local`
																to indicate system local time.

																If not set, `local` is used.

																[tz_database]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
																"""
						required: false
						type: string: examples: ["local", "America/New_York", "EST5EDT"]
					}
				}
			}
		}
	}
	framing: {
		description: """
			Framing configuration.

			Framing handles how events are separated when encoded in a raw byte form, where each event is
			a frame that must be prefixed, or delimited, in a way that marks where an event begins and
			ends within the byte stream.
			"""
		required: false
		type: object: options: {
			character_delimited: {
				description:   "Options for the character delimited decoder."
				relevant_when: "method = \"character_delimited\""
				required:      true
				type: object: options: {
					delimiter: {
						description: "The character that delimits byte sequences."
						required:    true
						type: ascii_char: {}
					}
					max_length: {
						description: """
																The maximum length of the byte buffer.

																This length does *not* include the trailing delimiter.

																By default, no maximum length is enforced. If events are malformed, this can lead to
																additional resource usage as events continue to be buffered in memory, and can potentially
																lead to memory exhaustion in extreme cases.

																If there is a risk of processing malformed data, such as logs with user-controlled input,
																consider setting the maximum length to a reasonably large value as a safety net. This
																prevents processing from being unbounded.
																"""
						required: false
						type: uint: {}
					}
					oversized_action: {
						description: """
																The behavior when a frame exceeds `max_length`.

																When set to `drop` (the default), the entire oversized frame is discarded.
																When set to `truncate`, the frame is truncated to `max_length` bytes and the
																remainder is discarded up to the next delimiter.

																This option has no effect if `max_length` is not set.
																"""
						required: false
						type: string: {
							default: "drop"
							enum: {
								drop: "Drop the entire oversized frame."
								truncate: """
																			Truncate the frame to the maximum allowed size and emit the partial content.

																			The remainder of the oversized frame is discarded up to the next delimiter.
																			"""
							}
						}
					}
				}
			}
			chunked_gelf: {
				description:   "Options for the chunked GELF decoder."
				relevant_when: "method = \"chunked_gelf\""
				required:      false
				type: object: options: {
					decompression: {
						description: "Decompression configuration for GELF messages."
						required:    false
						type: string: {
							default: "Auto"
							enum: {
								Auto: "Automatically detect the decompression method based on the magic bytes of the message."
								Gzip: "Use Gzip decompression."
								None: "Do not decompress the message."
								Zlib: "Use Zlib decompression."
							}
						}
					}
					max_length: {
						description: """
																The maximum length of a single GELF message, in bytes. Messages longer than this length are
																dropped. If this option is not set, the decoder does not limit the length of messages and
																the per-message memory is unbounded.

																**Note**: A message can be composed of multiple chunks, and this limit applies to the whole
																message, not to individual chunks.

																This limit takes into account only the message payload. GELF header bytes are excluded from the calculation.
																The message payload is the concatenation of all chunk payloads.
																"""
						required: false
						type: uint: {}
					}
					pending_messages_limit: {
						description: """
																The maximum number of pending incomplete messages. If this limit is reached, the decoder starts
																dropping chunks of new messages, ensuring the memory usage of the decoder's state is bounded.
																If this option is not set, the decoder does not limit the number of pending messages and the memory usage
																of its messages buffer can grow unbounded. This matches Graylog Server's behavior.
																"""
						required: false
						type: uint: {}
					}
					timeout_secs: {
						description: """
																The timeout, in seconds, for a message to be fully received. If the timeout is reached, the
																decoder drops all received chunks for the timed-out message.
																"""
						required: false
						type: float: default: 5.0
					}
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
			max_frame_length: {
				description:   "Maximum frame length"
				relevant_when: "method = \"varint_length_delimited\""
				required:      false
				type: uint: default: 8388608
			}
			method: {
				description: "The framing method."
				required:    false
				type: string: {
					default: "newline_delimited"
					enum: {
						bytes:               "Byte frames are passed through as-is according to the underlying I/O boundaries (for example, split between messages or stream segments)."
						character_delimited: "Byte frames which are delimited by a chosen character."
						chunked_gelf: """
															Byte frames which are chunked GELF messages.

															[chunked_gelf]: https://go2docs.graylog.org/current/getting_in_log_data/gelf.html
															"""
						length_delimited:  "Byte frames which are prefixed by an unsigned big-endian 32-bit integer indicating the length."
						newline_delimited: "Byte frames which are delimited by a newline character."
						octet_counting: """
															Byte frames according to the [octet counting][octet_counting] format.

															[octet_counting]: https://tools.ietf.org/html/rfc6587#section-3.4.1
															"""
						varint_length_delimited: """
															Byte frames which are prefixed by a varint indicating the length.
															This is compatible with protobuf's length-delimited encoding.
															"""
					}
				}
			}
			newline_delimited: {
				description:   "Options for the newline delimited decoder."
				relevant_when: "method = \"newline_delimited\""
				required:      false
				type: object: options: {
					max_length: {
						description: """
																The maximum length of the byte buffer.

																This length does *not* include the trailing delimiter.

																By default, no maximum length is enforced. If events are malformed, this can lead to
																additional resource usage as events continue to be buffered in memory, and can potentially
																lead to memory exhaustion in extreme cases.

																If there is a risk of processing malformed data, such as logs with user-controlled input,
																consider setting the maximum length to a reasonably large value as a safety net. This
																prevents processing from being unbounded.
																"""
						required: false
						type: uint: {}
					}
					oversized_action: {
						description: """
																The behavior when a line exceeds `max_length`.

																When set to `drop` (the default), the entire oversized line is discarded.
																When set to `truncate`, the line is truncated to `max_length` bytes and the
																remainder is discarded up to the next newline.

																This option has no effect if `max_length` is not set.
																"""
						required: false
						type: string: {
							default: "drop"
							enum: {
								drop: "Drop the entire oversized frame."
								truncate: """
																			Truncate the frame to the maximum allowed size and emit the partial content.

																			The remainder of the oversized frame is discarded up to the next delimiter.
																			"""
							}
						}
					}
				}
			}
			octet_counting: {
				description:   "Options for the octet counting decoder."
				relevant_when: "method = \"octet_counting\""
				required:      false
				type: object: options: max_length: {
					description: "The maximum length of the byte buffer."
					required:    false
					type: uint: {}
				}
			}
		}
	}
	multiline: {
		description: """
			Multiline aggregation configuration.

			If not specified, multiline aggregation is disabled.
			"""
		required: false
		type: object: options: {
			condition_pattern: {
				description: """
					Regular expression pattern that is used to determine whether or not more lines should be read.

					This setting must be configured in conjunction with `mode`.
					"""
				required: true
				type: string: examples: ["^[\\s]+", "\\\\$", "^(INFO|ERROR) ", ";$"]
			}
			mode: {
				description: """
					Aggregation mode.

					This setting must be configured in conjunction with `condition_pattern`.
					"""
				required: true
				type: string: enum: {
					continue_past: """
						All consecutive lines matching this pattern, plus one additional line, are included in the group.

						This is useful in cases where a log message ends with a continuation marker, such as a backslash, indicating
						that the following line is part of the same message.
						"""
					continue_through: """
						All consecutive lines matching this pattern are included in the group.

						The first line (the line that matched the start pattern) does not need to match the `ContinueThrough` pattern.

						This is useful in cases such as a Java stack trace, where some indicator in the line (such as a leading
						whitespace) indicates that it is an extension of the proceeding line.
						"""
					halt_before: """
						All consecutive lines not matching this pattern are included in the group.

						This is useful where a log line contains a marker indicating that it begins a new message.
						"""
					halt_with: """
						All consecutive lines, up to and including the first line matching this pattern, are included in the group.

						This is useful where a log line ends with a termination marker, such as a semicolon.
						"""
				}
			}
			start_pattern: {
				description: "Regular expression pattern that is used to match the start of a new message."
				required:    true
				type: string: examples: ["^[\\s]+", "\\\\$", "^(INFO|ERROR) ", ";$"]
			}
			timeout_ms: {
				description: """
					The maximum amount of time to wait for the next additional line, in milliseconds.

					Once this timeout is reached, the buffered message is guaranteed to be flushed, even if incomplete.
					"""
				required: true
				type: uint: {
					examples: [1000, 600000]
					unit: "milliseconds"
				}
			}
		}
	}
	queue: {
		description: "Configuration options for the Storage Queue."
		required:    false
		type: object: options: {
			client_concurrency: {
				description: """
					Number of concurrent tasks to create for polling the queue for messages.

					Defaults to the number of available CPUs on the system.

					Should not typically need to be changed, but it can sometimes be beneficial to raise this
					value when there is a high rate of messages being pushed into the queue and the blobs
					being fetched are small. In these cases, system resources may not be fully utilized
					without fetching more messages per second, as the queue message consumption rate affects
					the blob retrieval rate.
					"""
				required: false
				type: uint: {
					examples: [5]
					unit: "tasks"
				}
			}
			delete_failed_message: {
				description: """
					Whether to delete non-retryable messages.

					If a message is rejected by the sink and not retryable, it is deleted from the queue.
					With no dead-letter queue support, setting this to `false` means rejected messages are
					redelivered indefinitely.
					"""
				required: false
				type: bool: default: true
			}
			delete_message: {
				description: """
					Whether to delete the message once it is processed.

					It can be useful to set this to `false` for debugging or during the initial setup.
					"""
				required: false
				type: bool: default: true
			}
			max_number_of_messages: {
				description: """
					Maximum number of messages to poll from the queue in a batch.

					Should be set to a smaller value when the blobs are large to help prevent the ingestion
					of one blob from causing the others to exceed the `visibility_timeout_secs`. Valid
					values are 1 - 32.
					"""
				required: false
				type: uint: {
					default: 10
					examples: [1]
				}
			}
			poll_secs: {
				description: """
					Maximum time to wait between polls of the queue when it is empty, in seconds.

					Azure Storage Queues have no server-side long polling, so an exponential client-side
					backoff (starting at one second) is applied between empty polls, capped at this value.
					Polling resumes immediately whenever a poll returns at least one message.

					Must be at least `1`.
					"""
				required: false
				type: uint: {
					default: 15
					unit:    "seconds"
				}
			}
			queue_name: {
				description: """
					The name of the Storage Queue that receives the `Microsoft.Storage.BlobCreated`
					notifications from the Event Grid subscription.

					This is a queue name, not a URL; the full URL is derived from the queue service endpoint.
					"""
				required: true
				type: string: examples: ["vector-blob-events"]
			}
			visibility_timeout_secs: {
				description: """
					The visibility timeout to use for messages, in seconds.

					This controls how long a message is left unavailable after it is received. If a message
					is received, and takes longer than `visibility_timeout_secs` to process and delete the
					message from the queue, it is made available again for another consumer.

					This can happen if there is an issue between consuming a message and deleting it.
					"""
				required: false
				type: uint: {
					default: 300
					unit:    "seconds"
				}
			}
		}
	}
	queue_endpoint: {
		description: """
			The Azure Queue Storage service endpoint.

			By default the queue endpoint is derived from `account_name` or the connection string.
			"""
		required: false
		type: string: examples: ["https://mylogstorage.queue.core.windows.net/"]
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type: object: options: ca_file: {
			description: """
				Absolute path to an additional CA certificate file.

				The certificate must be in PEM (X.509) format.
				"""
			required: false
			type: string: examples: ["/path/to/certificate_authority.crt"]
		}
	}
}
