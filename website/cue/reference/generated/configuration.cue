package metadata

generated: configuration: {
	configuration: {
		api: {
			type: object: options: {
				address: {
					type: string: {
						default: "127.0.0.1:8686"
						examples: ["0.0.0.0:8686", "127.0.0.1:1234"]
					}
					description: """
						The network address to which the API should bind. If you're running
						Vector in a Docker container, bind to `0.0.0.0`. Otherwise
						the API will not be exposed outside the container.
						"""
					common:   true
					required: false
				}
				enabled: {
					type: bool: default: false
					description: "Whether the API is enabled for this Vector instance."
					common:      true
					required:    false
				}
			}
			description: "API options."
			warnings: ["The API currently does not support authentication. Only enable it in isolated environments or for debugging. It must not be exposed to untrusted clients."]
			group: "api"
		}
		enrichment_tables: {
			type: object: options: "*": {
				type: object: options: {
					graph: {
						type: object: options: {
							edge_attributes: {
								type: object: {
									options: "*": {
										type: object: {
											options: "*": {
												type: string: {}
												required:    true
												description: "A single graph edge attribute in graphviz DOT language."
											}
											examples: [{
												color: "red"
												label: "Example Edge"
												width: "5.0"
											}]
										}
										description: "A collection of graph edge attributes in graphviz DOT language, related to a single input component."
										required:    true
									}
									examples: [{
										example_input: {
											color: "red"
											label: "Example Edge"
											width: "5.0"
										}
									}]
								}
								description: """
																		Edge attributes to add to the edges linked to this component's node in resulting graph

																		They are added to the edge as provided
																		"""
								required: false
							}
							node_attributes: {
								type: object: {
									options: "*": {
										type: string: {}
										required:    true
										description: "A single graph node attribute in graphviz DOT language."
									}
									examples: [{
										color: "red"
										name:  "Example Node"
										width: "5.0"
									}]
								}
								description: """
																		Node attributes to add to this component's node in resulting graph

																		They are added to the node as provided
																		"""
								required: false
							}
						}
						description: """
														Extra graph configuration

														Configure output for component when generated with graph command
														"""
						required: false
					}
					inputs: {
						type: array: {
							items: type: string: examples: ["my-source-or-transform-id", "prefix-*"]
							default: []
						}
						description: """
														A list of upstream [source][sources] or [transform][transforms] IDs.

														Wildcards (`*`) are supported.

														See [configuration][configuration] for more info.

														[sources]: https://vector.dev/docs/reference/configuration/sources/
														[transforms]: https://vector.dev/docs/reference/configuration/transforms/
														[configuration]: https://vector.dev/docs/reference/configuration/
														"""
						required: false
					}
					file: {
						type: object: options: {
							encoding: {
								type: object: options: {
									delimiter: {
										type: string: default: ","
										description: "The delimiter used to separate fields in each row of the CSV file."
										required:    false
									}
									include_headers: {
										type: bool: default: true
										description: """
																						Whether or not the file contains column headers.

																						When set to `true`, the first row of the CSV file will be read as the header row, and
																						the values will be used for the names of each column. This is the default behavior.

																						When set to `false`, columns are referred to by their numerical index.
																						"""
										required: false
									}
									type: {
										required: true
										type: string: enum: csv: """
																									Decodes the file as a [CSV][csv] (comma-separated values) file.

																									[csv]: https://wikipedia.org/wiki/Comma-separated_values
																									"""
										description: "File encoding type."
									}
								}
								description: "File encoding configuration."
								required:    true
							}
							path: {
								type: string: {}
								description: """
																		The path of the enrichment table file.

																		Currently, only [CSV][csv] files are supported.

																		[csv]: https://en.wikipedia.org/wiki/Comma-separated_values
																		"""
								required: true
							}
						}
						description:   "File-specific settings."
						required:      true
						relevant_when: "type = \"file\""
					}
					schema: {
						type: object: options: "*": {
							type: string: {}
							required:    true
							description: "Represents mapped log field names and types."
						}
						description: """
														Key/value pairs representing mapped log field names and types.

														This is used to coerce log fields from strings into their proper types. The available types are listed in the `Types` list below.

														Timestamp coercions need to be prefaced with `timestamp|`, for example `"timestamp|%F"`. Timestamp specifiers can use either of the following:

														1. One of the built-in-formats listed in the `Timestamp Formats` table below.
														2. The [time format specifiers][chrono_fmt] from Rust’s `chrono` library.

														Types

														- **`bool`**
														- **`string`**
														- **`float`**
														- **`integer`**
														- **`date`**
														- **`timestamp`** (see the table below for formats)

														Timestamp Formats

														| Format               | Description                                                                      | Example                          |
														|----------------------|----------------------------------------------------------------------------------|----------------------------------|
														| `%F %T`              | `YYYY-MM-DD HH:MM:SS`                                                            | `2020-12-01 02:37:54`            |
														| `%v %T`              | `DD-Mmm-YYYY HH:MM:SS`                                                           | `01-Dec-2020 02:37:54`           |
														| `%FT%T`              | [ISO 8601][iso8601]/[RFC 3339][rfc3339], without time zone                       | `2020-12-01T02:37:54`            |
														| `%FT%TZ`             | [ISO 8601][iso8601]/[RFC 3339][rfc3339], UTC                                     | `2020-12-01T09:37:54Z`           |
														| `%+`                 | [ISO 8601][iso8601]/[RFC 3339][rfc3339], UTC, with time zone                     | `2020-12-01T02:37:54-07:00`      |
														| `%a, %d %b %Y %T`    | [RFC 822][rfc822]/[RFC 2822][rfc2822], without time zone                         | `Tue, 01 Dec 2020 02:37:54`      |
														| `%a %b %e %T %Y`     | [ctime][ctime] format                                                            | `Tue Dec 1 02:37:54 2020`        |
														| `%s`                 | [UNIX timestamp][unix_ts]                                                        | `1606790274`                     |
														| `%a %d %b %T %Y`     | [date][date] command, without time zone                                          | `Tue 01 Dec 02:37:54 2020`       |
														| `%a %d %b %T %Z %Y`  | [date][date] command, with time zone                                             | `Tue 01 Dec 02:37:54 PST 2020`   |
														| `%a %d %b %T %z %Y`  | [date][date] command, with numeric time zone                                     | `Tue 01 Dec 02:37:54 -0700 2020` |
														| `%a %d %b %T %#z %Y` | [date][date] command, with numeric time zone (minutes can be missing or present) | `Tue 01 Dec 02:37:54 -07 2020`   |

														[date]: https://man7.org/linux/man-pages/man1/date.1.html
														[ctime]: https://www.cplusplus.com/reference/ctime
														[unix_ts]: https://en.wikipedia.org/wiki/Unix_time
														[rfc822]: https://tools.ietf.org/html/rfc822#section-5
														[rfc2822]: https://tools.ietf.org/html/rfc2822#section-3.3
														[iso8601]: https://en.wikipedia.org/wiki/ISO_8601
														[rfc3339]: https://tools.ietf.org/html/rfc3339
														[chrono_fmt]: https://docs.rs/chrono/latest/chrono/format/strftime/index.html#specifiers
														"""
						required:      false
						relevant_when: "type = \"file\""
					}
					flush_interval: {
						type: uint: {}
						description: """
														The interval used for making writes visible in the table.
														Longer intervals might get better performance,
														but there is a longer delay before the data is visible in the table.
														Since every TTL scan makes its changes visible, only use this value
														if it is shorter than the `scan_interval`.

														By default, all writes are made visible immediately.
														"""
						required:      false
						relevant_when: "type = \"memory\""
					}
					internal_metrics: {
						type: object: options: include_key_tag: {
							type: bool: default: false
							description: """
																		Determines whether to include the key tag on internal metrics.

																		This is useful for distinguishing between different keys while monitoring. However, the tag's
																		cardinality is unbounded.
																		"""
							required: false
						}
						description:   "Configuration of internal metrics"
						required:      false
						relevant_when: "type = \"memory\""
					}
					max_byte_size: {
						type: uint: {}
						description: """
														Maximum size of the table in bytes. All insertions that make
														this table bigger than the maximum size are rejected.

														By default, there is no size limit.
														"""
						required:      false
						relevant_when: "type = \"memory\""
					}
					scan_interval: {
						type: uint: default: 30
						description: """
														The scan interval used to look for expired records. This is provided
														as an optimization to ensure that TTL is updated, but without doing
														too many cache scans.
														"""
						required:      false
						relevant_when: "type = \"memory\""
					}
					source_config: {
						type: object: options: {
							export_batch_size: {
								type: uint: {}
								description: """
																		Batch size for data exporting. Used to prevent exporting entire table at
																		once and blocking the system.

																		By default, batches are not used and entire table is exported.
																		"""
								required: false
							}
							export_expired_items: {
								type: bool: default: false
								description: """
																		Set to true to export expired items via the `expired` output port.
																		Expired items ignore other settings and are exported as they are flushed from the table.
																		"""
								required: false
							}
							export_interval: {
								type: uint: {}
								description: "Interval for exporting all data from the table when used as a source."
								required:    false
							}
							remove_after_export: {
								type: bool: default: false
								description: """
																		If set to true, all data will be removed from cache after exporting.
																		Only valid if used as a source and export_interval > 0

																		By default, export will not remove data from cache
																		"""
								required: false
							}
							source_key: {
								type: string: {}
								description: """
																		Key to use for this component when used as a source. This must be different from the
																		component key.
																		"""
								required: true
							}
						}
						description:   "Configuration for source functionality."
						required:      false
						relevant_when: "type = \"memory\""
					}
					ttl: {
						type: uint: default: 600
						description: """
														TTL (time-to-live in seconds) is used to limit the lifetime of data stored in the cache.
														When TTL expires, data behind a specific key in the cache is removed.
														TTL is reset when the key is replaced.
														"""
						required:      false
						relevant_when: "type = \"memory\""
					}
					ttl_field: {
						type: string: default: ""
						description:   "Field in the incoming value used as the TTL override."
						required:      false
						relevant_when: "type = \"memory\""
					}
					locale: {
						type: string: default: "en"
						description: """
														The locale to use when querying the database.

														MaxMind includes localized versions of some of the fields within their database, such as
														country name. This setting can control which of those localized versions are returned by the
														transform.

														More information on which portions of the geolocation data are localized, and what languages
														are available, can be found [here][locale_docs].

														[locale_docs]: https://support.maxmind.com/hc/en-us/articles/4414877149467-IP-Geolocation-Data#h_01FRRGRYTGZB29ERDBZCX3MR8Q
														"""
						required:      false
						relevant_when: "type = \"geoip\""
					}
					path: {
						type: string: {}
						description: """
														Path to the [MaxMind GeoIP2][geoip2] or [GeoLite2 binary city database file][geolite2]
														(**GeoLite2-City.mmdb**).

														Other databases, such as the country database, are not supported.
														`mmdb` enrichment table can be used for other databases.

														[geoip2]: https://dev.maxmind.com/geoip/geoip2/downloadable
														[geolite2]: https://dev.maxmind.com/geoip/geoip2/geolite2/#Download_Access
														"""
						required:      true
						relevant_when: "type = \"geoip\" or type = \"mmdb\""
					}
					type: {
						required: true
						type: string: enum: {
							file: "Exposes data from a static file as an enrichment table."
							memory: """
																	Exposes data from a memory cache as an enrichment table. The cache can be written to using
																	a sink.
																	"""
							geoip: """
																	Exposes data from a [MaxMind][maxmind] [GeoIP2][geoip2] database as an enrichment table.

																	[maxmind]: https://www.maxmind.com/
																	[geoip2]: https://www.maxmind.com/en/geoip2-databases
																	"""
							mmdb: """
																	Exposes data from a [MaxMind][maxmind] database as an enrichment table.

																	[maxmind]: https://www.maxmind.com/
																	"""
						}
						description: "enrichment table type"
					}
				}
				description: "An enrichment table."
				required:    true
			}
			description: "All configured enrichment tables."
			group:       "pipeline_components"
		}
		healthchecks: {
			type: object: options: {
				enabled: {
					type: bool: default: true
					description: """
						Whether or not healthchecks are enabled for all sinks.

						Can be overridden on a per-sink basis.
						"""
					required: false
				}
				require_healthy: {
					type: bool: default: false
					description: """
						Whether or not to require a sink to report as being healthy during startup.

						When enabled and a sink reports not being healthy, Vector will exit during start-up.

						Can be alternatively set, and overridden by, the `--require-healthy` command-line flag.
						"""
					required: false
				}
			}
			description: "Healthcheck options."
			group:       "global_options"
		}
		schema: {
			type: object: options: {
				enabled: {
					type: bool: default: false
					description: """
						When enabled, Vector tracks the schema (field types and structure) of events as they flow
						from sources through transforms to sinks. This allows Vector to understand what data each
						component receives and produces.
						"""
					required: false
				}
				log_namespace: {
					type: bool: {}
					description: """
						Controls how metadata is stored in log events.

						When set to `false` (legacy mode), metadata fields like `host`, `timestamp`, and `source_type`
						are stored as top-level fields alongside your log data.

						When set to `true` (Vector namespace mode), metadata is stored in a separate metadata namespace,
						keeping it distinct from your actual log data.

						See the [Log Namespacing guide](/guides/level-up/log_namespace/) for detailed information
						about when to use Vector namespace mode and how to migrate from legacy mode.
						"""
					required: false
				}
				validation: {
					type: bool: default: false
					description: """
						When enabled, Vector validates that events flowing into each sink match the schema
						requirements of that sink. If a sink requires certain fields or types that are missing
						from the incoming events, Vector will report an error during configuration validation.

						This helps catch pipeline configuration errors early, before runtime.
						"""
					required: false
				}
			}
			description: """
				Schema options.

				**Note:** The `enabled` and `validation` options are experimental and should only be enabled if you
				understand the limitations. While the infrastructure exists for schema tracking and validation, the
				full vision of automatic semantic field mapping and comprehensive schema enforcement was never fully
				realized.

				If you encounter issues with these features, please [report them here](https://github.com/vectordotdev/vector/issues/new?template=bug.yml).
				"""
			group: "schema"
		}
		secret: {
			type: object: options: "*": {
				type: object: options: {
					path: {
						type: string: {}
						description:   "File path to read secrets from."
						required:      true
						relevant_when: "type = \"file\" or type = \"directory\""
					}
					remove_trailing_whitespace: {
						type: bool: default: false
						description:   "Remove trailing whitespace from file contents."
						required:      false
						relevant_when: "type = \"directory\""
					}
					command: {
						type: array: items: type: string: {}
						description: """
														Command arguments to execute.

														The path to the script or binary must be the first argument.
														"""
						required:      true
						relevant_when: "type = \"exec\""
					}
					protocol: {
						type: object: options: {
							backend_config: {
								type: "*": {}
								description: """
																		The configuration to pass to the secrets executable. This is the `config` field in the
																		backend request. Refer to the documentation of your `backend_type `to see which options
																		are required to be set.
																		"""
								required:      false
								relevant_when: "version = \"v1_1\""
							}
							backend_type: {
								type: string: {}
								description:   "The name of the backend. This is `type` field in the backend request."
								required:      true
								relevant_when: "version = \"v1_1\""
							}
							version: {
								required: false
								type: string: {
									enum: {
										v1:   "Expect the command to fetch the configuration options itself."
										v1_1: "Configuration options to the command are to be curried upon each request."
									}
									default: "v1"
								}
								description: "The protocol version."
							}
						}
						description:   "Settings for the protocol between Vector and the secrets executable."
						required:      false
						relevant_when: "type = \"exec\""
					}
					timeout: {
						type: uint: default: 5
						description:   "The timeout, in seconds, to wait for the command to complete."
						required:      false
						relevant_when: "type = \"exec\""
					}
					auth: {
						type: object: options: {
							access_key_id: {
								type: string: examples: ["AKIAIOSFODNN7EXAMPLE"]
								description: "The AWS access key ID."
								required:    true
							}
							assume_role: {
								type: string: examples: ["arn:aws:iam::123456789098:role/my_role"]
								description: """
																		The ARN of an [IAM role][iam_role] to assume.

																		[iam_role]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles.html
																		"""
								required: true
							}
							external_id: {
								type: string: examples: ["randomEXAMPLEidString"]
								description: """
																		The optional unique external ID in conjunction with role to assume.

																		[external_id]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles_create_for-user_externalid.html
																		"""
								required: false
							}
							region: {
								type: string: examples: ["us-west-2"]
								description: """
																		The [AWS region][aws_region] to send STS requests to.

																		If not set, this defaults to the configured region
																		for the service itself.

																		[aws_region]: https://docs.aws.amazon.com/general/latest/gr/rande.html#regional-endpoints
																		"""
								required: false
							}
							secret_access_key: {
								type: string: examples: ["wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"]
								description: "The AWS secret access key."
								required:    true
							}
							session_name: {
								type: string: examples: ["vector-indexer-role"]
								description: """
																		The optional [RoleSessionName][role_session_name] is a unique session identifier for your assumed role.

																		Should be unique per principal or reason.
																		If not set, the session name is autogenerated like assume-role-provider-1736428351340

																		[role_session_name]: https://docs.aws.amazon.com/STS/latest/APIReference/API_AssumeRole.html
																		"""
								required: false
							}
							session_token: {
								type: string: examples: ["AQoDYXdz...AQoDYXdz..."]
								description: """
																		The AWS session token.
																		See [AWS temporary credentials](https://docs.aws.amazon.com/IAM/latest/UserGuide/id_credentials_temp_use-resources.html)
																		"""
								required: false
							}
							credentials_file: {
								type: string: examples: ["/my/aws/credentials"]
								description: "Path to the credentials file."
								required:    true
							}
							profile: {
								type: string: {
									default: "default"
									examples: ["develop"]
								}
								description: """
																		The credentials profile to use.

																		Used to select AWS credentials from a provided credentials file.
																		"""
								required: false
							}
							imds: {
								type: object: options: {
									connect_timeout_seconds: {
										type: uint: {
											default: 1
											unit:    "seconds"
										}
										description: "Connect timeout for IMDS."
										required:    false
									}
									max_attempts: {
										type: uint: default: 4
										description: "Number of IMDS retries for fetching tokens and metadata."
										required:    false
									}
									read_timeout_seconds: {
										type: uint: {
											default: 1
											unit:    "seconds"
										}
										description: "Read timeout for IMDS."
										required:    false
									}
								}
								description: "Configuration for authenticating with AWS through IMDS."
								required:    false
							}
							load_timeout_secs: {
								type: uint: {
									examples: [30]
									unit: "seconds"
								}
								description: """
																		Timeout for successfully loading any credentials, in seconds.

																		Relevant when the default credentials chain or `assume_role` is used.
																		"""
								required: false
							}
						}
						description:   "Configuration of the authentication strategy for interacting with AWS services."
						required:      false
						relevant_when: "type = \"aws_secrets_manager\""
					}
					secret_id: {
						type: string: {}
						description:   "ID of the secret to resolve."
						required:      true
						relevant_when: "type = \"aws_secrets_manager\""
					}
					tls: {
						type: object: options: {
							alpn_protocols: {
								type: array: items: type: string: examples: ["h2"]
								description: """
																		Sets the list of supported ALPN protocols.

																		Declare the supported ALPN protocols, which are used during negotiation with a peer. They are prioritized in the order
																		that they are defined.
																		"""
								required: false
							}
							ca_file: {
								type: string: examples: ["/path/to/certificate_authority.crt"]
								description: """
																		Absolute path to an additional CA certificate file.

																		The certificate must be in the DER or PEM (X.509) format. Additionally, the certificate can be provided as an inline string in PEM format.
																		"""
								required: false
							}
							crt_file: {
								type: string: examples: ["/path/to/host_certificate.crt"]
								description: """
																		Absolute path to a certificate file used to identify this server.

																		The certificate must be in DER, PEM (X.509), or PKCS#12 format. Additionally, the certificate can be provided as
																		an inline string in PEM format.

																		If this is set _and_ is not a PKCS#12 archive, `key_file` must also be set.
																		"""
								required: false
							}
							key_file: {
								type: string: examples: ["/path/to/host_certificate.key"]
								description: """
																		Absolute path to a private key file used to identify this server.

																		The key must be in DER or PEM (PKCS#8) format. Additionally, the key can be provided as an inline string in PEM format.
																		"""
								required: false
							}
							key_pass: {
								type: string: examples: ["${KEY_PASS_ENV_VAR}", "PassWord1"]
								description: """
																		Passphrase used to unlock the encrypted key file.

																		This has no effect unless `key_file` is set.
																		"""
								required: false
							}
							server_name: {
								type: string: examples: ["www.example.com"]
								description: """
																		Server name to use when using Server Name Indication (SNI).

																		Only relevant for outgoing connections.
																		"""
								required: false
							}
							verify_certificate: {
								type: bool: {}
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
							}
							verify_hostname: {
								type: bool: {}
								description: """
																		Enables hostname verification.

																		If enabled, the hostname used to connect to the remote host must be present in the TLS certificate presented by
																		the remote host, either as the Common Name or as an entry in the Subject Alternative Name extension.

																		Only relevant for outgoing connections.

																		Do NOT set this to `false` unless you understand the risks of not verifying the remote hostname.
																		"""
								required: false
							}
						}
						description:   "TLS configuration."
						required:      false
						relevant_when: "type = \"aws_secrets_manager\""
					}
					endpoint: {
						type: string: examples: ["http://127.0.0.0:5000/path/to/service"]
						description:   "Custom endpoint for use with AWS-compatible services."
						required:      false
						relevant_when: "type = \"aws_secrets_manager\""
					}
					region: {
						type: string: examples: ["us-east-1"]
						description: """
														The [AWS region][aws_region] of the target service.

														[aws_region]: https://docs.aws.amazon.com/general/latest/gr/rande.html#regional-endpoints
														"""
						required:      false
						relevant_when: "type = \"aws_secrets_manager\""
					}
					type: {
						required: true
						type: string: enum: {
							file:                "File."
							directory:           "Directory."
							exec:                "Exec."
							aws_secrets_manager: "AWS Secrets Manager."
						}
						description: "secret type"
					}
				}
				description: "A secret backend."
				common:      false
				required:    true
			}
			description: "All configured secrets backends."
			group:       "secrets"
		}
		sinks: {
			type: object: options: "*": {
				type: object: options: {
					buffer: {
						type: object: options: {
							when_full: {
								type: string: {
									enum: {
										block: """
																					Wait for free space in the buffer.

																					This applies backpressure up the topology, signalling that sources should slow down
																					the acceptance/consumption of events. This means that while no data is lost, data will pile
																					up at the edge.
																					"""
										drop_newest: """
																					Drops the event instead of waiting for free space in buffer.

																					The event will be intentionally dropped. This mode is typically used when performance is the
																					highest priority, and it is preferable to temporarily lose events rather than cause a
																					slowdown in the acceptance/consumption of events.
																					"""
									}
									default: "block"
								}
								description: "Event handling behavior when a buffer is full."
								required:    false
							}
							max_events: {
								type: uint: default: 500
								required:      false
								description:   "The maximum number of events allowed in the buffer."
								relevant_when: "type = \"memory\""
							}
							max_size: {
								type: uint: unit: "bytes"
								required: true
								description: """
																		The maximum allowed amount of allocated memory the buffer can hold.

																		If `type = "disk"` then must be at least ~256 megabytes (268435488 bytes).
																		"""
							}
							type: {
								required: false
								type: string: {
									enum: {
										memory: """
																					Events are buffered in memory.

																					This is more performant, but less durable. Data will be lost if Vector is restarted
																					forcefully or crashes.
																					"""
										disk: """
																					Events are buffered on disk.

																					This is less performant, but more durable. Data that has been synchronized to disk will not
																					be lost if Vector is restarted forcefully or crashes.

																					Data is synchronized to disk every 500ms.
																					"""
									}
									default: "memory"
								}
								description: "The type of buffer to use."
							}
						}
						description: """
														Configures the buffering behavior for this sink.

														More information about the individual buffer types, and buffer behavior, can be found in the
														[Buffering Model][buffering_model] section.

														[buffering_model]: /docs/architecture/buffering-model/
														"""
						required: false
					}
					graph: {
						type: object: options: {
							edge_attributes: {
								type: object: {
									options: "*": {
										type: object: {
											options: "*": {
												type: string: {}
												required:    true
												description: "A single graph edge attribute in graphviz DOT language."
											}
											examples: [{
												color: "red"
												label: "Example Edge"
												width: "5.0"
											}]
										}
										description: "A collection of graph edge attributes in graphviz DOT language, related to a single input component."
										required:    true
									}
									examples: [{
										example_input: {
											color: "red"
											label: "Example Edge"
											width: "5.0"
										}
									}]
								}
								description: """
																		Edge attributes to add to the edges linked to this component's node in resulting graph

																		They are added to the edge as provided
																		"""
								required: false
							}
							node_attributes: {
								type: object: {
									options: "*": {
										type: string: {}
										required:    true
										description: "A single graph node attribute in graphviz DOT language."
									}
									examples: [{
										color: "red"
										name:  "Example Node"
										width: "5.0"
									}]
								}
								description: """
																		Node attributes to add to this component's node in resulting graph

																		They are added to the node as provided
																		"""
								required: false
							}
						}
						description: """
														Extra graph configuration

														Configure output for component when generated with graph command
														"""
						required: false
					}
					healthcheck: {
						type: object: options: {
							enabled: {
								type: bool: default: true
								description: "Whether or not to check the health of the sink when Vector starts up."
								required:    false
							}
							timeout: {
								type: float: {
									default: 10.0
									unit:    "seconds"
								}
								description: "Timeout duration for healthcheck in seconds."
								required:    false
							}
							uri: {
								type: string: {}
								description: """
																		The full URI to make HTTP healthcheck requests to.

																		This must be a valid URI, which requires at least the scheme and host. All other
																		components -- port, path, etc -- are allowed as well.
																		"""
								required: false
							}
						}
						description: "Healthcheck configuration."
						required:    false
					}
					inputs: {
						type: array: items: type: string: examples: ["my-source-or-transform-id", "prefix-*"]
						description: """
														A list of upstream [source][sources] or [transform][transforms] IDs.

														Wildcards (`*`) are supported.

														See [configuration][configuration] for more info.

														[sources]: https://vector.dev/docs/reference/configuration/sources/
														[transforms]: https://vector.dev/docs/reference/configuration/transforms/
														[configuration]: https://vector.dev/docs/reference/configuration/
														"""
						required: true
					}
					proxy: {
						type: object: options: {
							enabled: {
								type: bool: default: true
								description: "Enables proxying support."
								required:    false
							}
							http: {
								type: string: examples: ["http://foo.bar:3128"]
								description: """
																		Proxy endpoint to use when proxying HTTP traffic.

																		Must be a valid URI string.
																		"""
								required: false
							}
							https: {
								type: string: examples: ["http://foo.bar:3128"]
								description: """
																		Proxy endpoint to use when proxying HTTPS traffic.

																		Must be a valid URI string.
																		"""
								required: false
							}
							no_proxy: {
								type: array: {
									items: type: string: examples: ["localhost", ".foo.bar", "*"]
									default: []
								}
								description: """
																		A list of hosts to avoid proxying.

																		Multiple patterns are allowed:

																		| Pattern             | Example match                                                               |
																		| ------------------- | --------------------------------------------------------------------------- |
																		| Domain names        | `example.com` matches requests to `example.com`                     |
																		| Wildcard domains    | `.example.com` matches requests to `example.com` and its subdomains |
																		| IP addresses        | `127.0.0.1` matches requests to `127.0.0.1`                         |
																		| [CIDR][cidr] blocks | `192.168.0.0/16` matches requests to any IP addresses in this range     |
																		| Splat               | `*` matches all hosts                                                   |

																		[cidr]: https://en.wikipedia.org/wiki/Classless_Inter-Domain_Routing
																		"""
								required: false
							}
						}
						description: """
														Proxy configuration.

														Configure to proxy traffic through an HTTP(S) proxy when making external requests.

														Similar to common proxy configuration convention, you can set different proxies
														to use based on the type of traffic being proxied. You can also set specific hosts that
														should not be proxied.
														"""
						required: false
					}
				}
				description: "A sink."
				required:    true
			}
			description: "All configured sinks."
			group:       "pipeline_components"
		}
		sources: {
			type: object: options: "*": {
				type: object: options: {
					graph: {
						type: object: options: {
							edge_attributes: {
								type: object: {
									options: "*": {
										type: object: {
											options: "*": {
												type: string: {}
												required:    true
												description: "A single graph edge attribute in graphviz DOT language."
											}
											examples: [{
												color: "red"
												label: "Example Edge"
												width: "5.0"
											}]
										}
										description: "A collection of graph edge attributes in graphviz DOT language, related to a single input component."
										required:    true
									}
									examples: [{
										example_input: {
											color: "red"
											label: "Example Edge"
											width: "5.0"
										}
									}]
								}
								description: """
																		Edge attributes to add to the edges linked to this component's node in resulting graph

																		They are added to the edge as provided
																		"""
								required: false
							}
							node_attributes: {
								type: object: {
									options: "*": {
										type: string: {}
										required:    true
										description: "A single graph node attribute in graphviz DOT language."
									}
									examples: [{
										color: "red"
										name:  "Example Node"
										width: "5.0"
									}]
								}
								description: """
																		Node attributes to add to this component's node in resulting graph

																		They are added to the node as provided
																		"""
								required: false
							}
						}
						description: """
														Extra graph configuration

														Configure output for component when generated with graph command
														"""
						required: false
					}
					proxy: {
						type: object: options: {
							enabled: {
								type: bool: default: true
								description: "Enables proxying support."
								required:    false
							}
							http: {
								type: string: examples: ["http://foo.bar:3128"]
								description: """
																		Proxy endpoint to use when proxying HTTP traffic.

																		Must be a valid URI string.
																		"""
								required: false
							}
							https: {
								type: string: examples: ["http://foo.bar:3128"]
								description: """
																		Proxy endpoint to use when proxying HTTPS traffic.

																		Must be a valid URI string.
																		"""
								required: false
							}
							no_proxy: {
								type: array: {
									items: type: string: examples: ["localhost", ".foo.bar", "*"]
									default: []
								}
								description: """
																		A list of hosts to avoid proxying.

																		Multiple patterns are allowed:

																		| Pattern             | Example match                                                               |
																		| ------------------- | --------------------------------------------------------------------------- |
																		| Domain names        | `example.com` matches requests to `example.com`                     |
																		| Wildcard domains    | `.example.com` matches requests to `example.com` and its subdomains |
																		| IP addresses        | `127.0.0.1` matches requests to `127.0.0.1`                         |
																		| [CIDR][cidr] blocks | `192.168.0.0/16` matches requests to any IP addresses in this range     |
																		| Splat               | `*` matches all hosts                                                   |

																		[cidr]: https://en.wikipedia.org/wiki/Classless_Inter-Domain_Routing
																		"""
								required: false
							}
						}
						description: """
														Proxy configuration.

														Configure to proxy traffic through an HTTP(S) proxy when making external requests.

														Similar to common proxy configuration convention, you can set different proxies
														to use based on the type of traffic being proxied. You can also set specific hosts that
														should not be proxied.
														"""
						required: false
					}
				}
				description: "A source."
				required:    true
			}
			description: "All configured sources."
			group:       "pipeline_components"
		}
		transforms: {
			type: object: options: "*": {
				type: object: options: {
					graph: {
						type: object: options: {
							edge_attributes: {
								type: object: {
									options: "*": {
										type: object: {
											options: "*": {
												type: string: {}
												required:    true
												description: "A single graph edge attribute in graphviz DOT language."
											}
											examples: [{
												color: "red"
												label: "Example Edge"
												width: "5.0"
											}]
										}
										description: "A collection of graph edge attributes in graphviz DOT language, related to a single input component."
										required:    true
									}
									examples: [{
										example_input: {
											color: "red"
											label: "Example Edge"
											width: "5.0"
										}
									}]
								}
								description: """
																		Edge attributes to add to the edges linked to this component's node in resulting graph

																		They are added to the edge as provided
																		"""
								required: false
							}
							node_attributes: {
								type: object: {
									options: "*": {
										type: string: {}
										required:    true
										description: "A single graph node attribute in graphviz DOT language."
									}
									examples: [{
										color: "red"
										name:  "Example Node"
										width: "5.0"
									}]
								}
								description: """
																		Node attributes to add to this component's node in resulting graph

																		They are added to the node as provided
																		"""
								required: false
							}
						}
						description: """
														Extra graph configuration

														Configure output for component when generated with graph command
														"""
						required: false
					}
					inputs: {
						type: array: items: type: string: examples: ["my-source-or-transform-id", "prefix-*"]
						description: """
														A list of upstream [source][sources] or [transform][transforms] IDs.

														Wildcards (`*`) are supported.

														See [configuration][configuration] for more info.

														[sources]: https://vector.dev/docs/reference/configuration/sources/
														[transforms]: https://vector.dev/docs/reference/configuration/transforms/
														[configuration]: https://vector.dev/docs/reference/configuration/
														"""
						required: true
					}
				}
				description: "A transform."
				required:    true
			}
			description: "All configured transforms."
			group:       "pipeline_components"
		}
		acknowledgements: {
			type: object: options: enabled: {
				type: bool: {}
				description: """
					Controls whether or not end-to-end acknowledgements are enabled.

					When enabled for a sink, any source that supports end-to-end
					acknowledgements that is connected to that sink waits for events
					to be acknowledged by **all connected sinks** before acknowledging them at the source.

					Enabling or disabling acknowledgements at the sink level takes precedence over any global
					[`acknowledgements`][global_acks] configuration.

					[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
					"""
				required: false
			}
			description: """
				Controls how acknowledgements are handled for all sinks by default.

				See [End-to-end Acknowledgements][e2e_acks] for more information on how Vector handles event
				acknowledgement.

				[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
				"""
			common:   true
			required: false
			group:    "global_options"
		}
		buffer_utilization_ewma_half_life_seconds: {
			type: float: {}
			description: """
				The half-life, in seconds, for the exponential weighted moving average (EWMA) of source
				and transform buffer utilization metrics.

				This controls how quickly the `*_buffer_utilization_mean` gauges respond to new
				observations. Longer half-lives retain more of the previous value, leading to slower
				adjustments.

				- Lower values (< 1): Metrics update quickly but may be volatile
				- Default (5): Balanced between responsiveness and stability
				- Higher values (> 5): Smooth, stable metrics that update slowly

				Adjust based on whether you need fast detection of buffer issues (lower)
				or want to see sustained trends without noise (higher).

				Must be greater than 0.
				"""
			group: "global_options"
		}
		data_dir: {
			type: string: default: "/var/lib/vector/"
			description: """
				The directory used for persisting Vector state data.

				This is the directory where Vector will store any state data, such as disk buffers, file
				checkpoints, and more.

				Vector must have write permissions to this directory.
				"""
			common: false
			group:  "global_options"
		}
		expire_metrics_per_metric_set: {
			type: array: items: type: object: options: {
				expire_secs: {
					type: float: examples: [60.0]
					description: """
						The amount of time, in seconds, that internal metrics will persist after having not been
						updated before they expire and are removed.

						Set this to a value larger than your `internal_metrics` scrape interval (default 5 minutes)
						so that metrics live long enough to be emitted and captured.
						"""
					required: true
				}
				labels: {
					type: object: options: {
						matchers: {
							type: array: items: type: object: options: {
								key: {
									type: string: {}
									description: "Metric key to look for."
									required:    true
								}
								value: {
									type: string: {}
									description:   "The exact metric label value."
									required:      true
									relevant_when: "type = \"exact\""
								}
								value_pattern: {
									type: string: {}
									description:   "Pattern to compare metric label value to."
									required:      true
									relevant_when: "type = \"regex\""
								}
								type: {
									required: true
									type: string: enum: {
										exact: "Looks for an exact match of one label key value pair."
										regex: "Compares label value with given key to the provided pattern."
									}
									description: "Metric label matcher type."
								}
							}
							description: "List of matchers to check."
							required:    true
						}
						type: {
							required: true
							type: string: enum: {
								any: "Checks that any of the provided matchers can be applied to given metric."
								all: "Checks that all of the provided matchers can be applied to given metric."
							}
							description: "Metric label group matcher type."
						}
					}
					description: "Labels to apply this expiration to. Ignores labels if not defined."
					required:    false
				}
				name: {
					type: object: options: {
						value: {
							type: string: {}
							description:   "The exact metric name."
							required:      true
							relevant_when: "type = \"exact\""
						}
						pattern: {
							type: string: {}
							description:   "Pattern to compare to."
							required:      true
							relevant_when: "type = \"regex\""
						}
						type: {
							required: true
							type: string: enum: {
								exact: "Only considers exact name matches."
								regex: "Compares metric name to the provided pattern."
							}
							description: "Metric name matcher type."
						}
					}
					description: "Metric name to apply this expiration to. Ignores metric name if not defined."
					required:    false
				}
			}
			description: """
				This allows configuring different expiration intervals for different metric sets.
				By default this is empty and any metric not matched by one of these sets will use
				the global default value, defined using `expire_metrics_secs`.
				"""
			group: "global_options"
		}
		expire_metrics_secs: {
			type: float: {}
			description: """
				The amount of time, in seconds, that internal metrics will persist after having not been
				updated before they expire and are removed.

				Set this to a value larger than your `internal_metrics` scrape interval (default 5 minutes)
				so metrics live long enough to be emitted and captured.
				"""
			common:   false
			required: false
			group:    "global_options"
		}
		latency_ewma_alpha: {
			type: float: {}
			description: """
				The alpha value for the exponential weighted moving average (EWMA) of transform latency
				metrics.

				This controls how quickly the `component_latency_mean_seconds` gauge responds to new
				observations. Values closer to 1.0 retain more of the previous value, leading to slower
				adjustments. The default value of 0.9 is equivalent to a "half life" of 6-7 measurements.

				Must be between 0 and 1 exclusively (0 < alpha < 1).
				"""
			group: "global_options"
		}
		log_schema: {
			type: object: options: {
				host_key: {
					type: string: default: ".host"
					description: """
						The name of the event field to treat as the host which sent the message.

						This field will generally represent a real host, or container, that generated the message,
						but is somewhat source-dependent.
						"""
					required: false
				}
				message_key: {
					type: string: default: ".message"
					description: """
						The name of the event field to treat as the event message.

						This would be the field that holds the raw message, such as a raw log line.
						"""
					required: false
				}
				metadata_key: {
					type: string: default: ".metadata"
					description: """
						The name of the event field to set the event metadata in.

						Generally, this field will be set by Vector to hold event-specific metadata, such as
						annotations by the `remap` transform when an error or abort is encountered.
						"""
					required: false
				}
				source_type_key: {
					type: string: default: ".source_type"
					description: """
						The name of the event field to set the source identifier in.

						This field will be set by the Vector source that the event was created in.
						"""
					required: false
				}
				timestamp_key: {
					type: string: default: ".timestamp"
					description: "The name of the event field to treat as the event timestamp."
					required:    false
				}
			}
			description: """
				Default log schema for all events.

				This is used if a component does not have its own specific log schema. All events use a log
				schema, whether or not the default is used, to assign event fields on incoming events.
				"""
			common:   false
			required: false
			group:    "schema"
		}
		metrics_storage_refresh_period: {
			type: float: {}
			description: """
				The interval, in seconds, at which the internal metrics cache for VRL is refreshed.
				This must be set to be able to access metrics in VRL functions.

				Higher values lead to stale metric values from `get_vector_metric`,
				`find_vector_metrics`, and `aggregate_vector_metrics` functions.
				"""
			group: "global_options"
		}
		proxy: {
			type: object: options: {
				enabled: {
					type: bool: default: true
					description: "Enables proxying support."
					required:    false
				}
				http: {
					type: string: examples: ["http://foo.bar:3128"]
					description: """
						Proxy endpoint to use when proxying HTTP traffic.

						Must be a valid URI string.
						"""
					required: false
				}
				https: {
					type: string: examples: ["http://foo.bar:3128"]
					description: """
						Proxy endpoint to use when proxying HTTPS traffic.

						Must be a valid URI string.
						"""
					required: false
				}
				no_proxy: {
					type: array: {
						items: type: string: examples: ["localhost", ".foo.bar", "*"]
						default: []
					}
					description: """
						A list of hosts to avoid proxying.

						Multiple patterns are allowed:

						| Pattern             | Example match                                                               |
						| ------------------- | --------------------------------------------------------------------------- |
						| Domain names        | `example.com` matches requests to `example.com`                     |
						| Wildcard domains    | `.example.com` matches requests to `example.com` and its subdomains |
						| IP addresses        | `127.0.0.1` matches requests to `127.0.0.1`                         |
						| [CIDR][cidr] blocks | `192.168.0.0/16` matches requests to any IP addresses in this range     |
						| Splat               | `*` matches all hosts                                                   |

						[cidr]: https://en.wikipedia.org/wiki/Classless_Inter-Domain_Routing
						"""
					required: false
				}
			}
			description: """
				Proxy configuration.

				Configure to proxy traffic through an HTTP(S) proxy when making external requests.

				Similar to common proxy configuration convention, you can set different proxies
				to use based on the type of traffic being proxied. You can also set specific hosts that
				should not be proxied.
				"""
			common:   false
			required: false
			group:    "global_options"
		}
		telemetry: {
			type: object: options: tags: {
				type: object: options: {
					emit_service: {
						type: bool: default: false
						description: """
														True if the `service` tag should be emitted
														in the `component_received_*` and `component_sent_*`
														telemetry.
														"""
						required: false
					}
					emit_source: {
						type: bool: default: false
						description: """
														True if the `source` tag should be emitted
														in the `component_received_*` and `component_sent_*`
														telemetry.
														"""
						required: false
					}
				}
				description: "Configures whether to emit certain tags"
				required:    false
			}
			description: """
				Telemetry options.

				Determines whether `source` and `service` tags should be emitted with the
				`component_sent_*` and `component_received_*` events.
				"""
			common:   false
			required: false
			group:    "global_options"
		}
		timezone: {
			type: string: examples: ["local", "America/New_York", "EST5EDT"]
			description: """
				The name of the time zone to apply to timestamp conversions that do not contain an explicit time zone.

				The time zone name may be any name in the [TZ database][tzdb] or `local` to indicate system
				local time.

				Note that in Vector/VRL all timestamps are represented in UTC.

				[tzdb]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
				"""
			common: false
			group:  "global_options"
		}
		wildcard_matching: {
			type: string: enum: {
				strict:  "Strict matching (must match at least one existing input)"
				relaxed: "Relaxed matching (must match 0 or more inputs)"
			}
			description: """
				Set wildcard matching mode for inputs

				Setting this to "relaxed" allows configurations with wildcards that do not match any inputs
				to be accepted without causing an error.
				"""
			common:   false
			required: false
			group:    "global_options"
		}
	}
	groups: {
		global_options: {
			title:       "Global Options"
			description: "Global configuration options that apply to Vector as a whole."
			order:       1
		}
		pipeline_components: {
			title:       "Pipeline Components"
			description: "Configure sources, transforms, sinks, and enrichment tables for your observability pipeline."
			order:       2
		}
		api: {
			title:       "API"
			description: "Configure Vector's observability API."
			order:       3
		}
		schema: {
			title:       "Schema"
			description: "Configure Vector's internal schema system for type tracking and validation."
			order:       4
		}
		secrets: {
			title:       "Secrets"
			description: "Configure secrets management for secure configuration."
			order:       5
		}
	}
}
