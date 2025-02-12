package metadata

base: configuration: configuration: {
	enrichment_tables: {
		type: object: options: {
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
		description: """
			Configuration options for an [enrichment table](https://vector.dev/docs/reference/glossary/#enrichment-tables) to be used in a
			[`remap`](https://vector.dev/docs/reference/configuration/transforms/remap/) transform. Currently supported are:

			* [CSV](https://en.wikipedia.org/wiki/Comma-separated_values) files
			* [MaxMind](https://www.maxmind.com/en/home) databases
			* In-memory storage

			For the lookup in the enrichment tables to be as performant as possible, the data is indexed according
			to the fields that are used in the search. Note that indices can only be created for fields for which an
			exact match is used in the condition. For range searches, an index isn't used and the enrichment table
			drops back to a sequential scan of the data. A sequential scan shouldn't impact performance
			significantly provided that there are only a few possible rows returned by the exact matches in the
			condition. We don't recommend using a condition that uses only date range searches.
			"""
		common:   false
		required: false
	}
	secret: {
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
														If not set, session name will be autogenerated like assume-role-provider-1736428351340

														[role_session_name]: https://docs.aws.amazon.com/STS/latest/APIReference/API_AssumeRole.html
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
		description: """
			Configuration options to retrieve secrets from external backend in order to avoid storing secrets in plaintext
			in Vector config. Multiple backends can be configured. Use `SECRET[<backend_name>.<secret_key>]` to tell Vector to retrieve the secret. This placeholder is replaced by the secret
			retrieved from the relevant backend.

			When `type` is `exec`, the provided command will be run and provided a list of
			secrets to fetch, determined from the configuration file, on stdin as JSON in the format:

			```json
			{"version": "1.0", "secrets": ["secret1", "secret2"]}
			```

			The executable is expected to respond with the values of these secrets on stdout, also as JSON, in the format:

			```json
			{
			    "secret1": {"value": "secret_value", "error": null},
			    "secret2": {"value": null, "error": "could not fetch the secret"}
			}
			```
			If an `error` is returned for any secrets, or if the command exits with a non-zero status code,
			Vector will log the errors and exit.

			Otherwise, the secret must be a JSON text string with key/value pairs. For example:
			```json
			{
			    "username": "test",
			    "password": "example-password"
			}
			```

			If an error occurred while reading the file or retrieving the secrets, Vector logs the error and exits.

			Secrets are loaded when Vector starts or if Vector receives a `SIGHUP` signal triggering its
			configuration reload process.
			"""
		common:   false
		required: false
	}
	acknowledgements: {
		common: true
		description: """
			Controls how acknowledgements are handled for all sinks by default.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how Vector handles event
			acknowledgement.

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
	data_dir: {
		common: false
		description: """
			The directory used for persisting Vector state data.

			This is the directory where Vector will store any state data, such as disk buffers, file
			checkpoints, and more.

			Vector must have write permissions to this directory.
			"""
		required: false
		type: string: default: "/var/lib/vector/"
	}
	expire_metrics_secs: {
		common: false
		description: """
			The amount of time, in seconds, that internal metrics will persist after having not been
			updated before they expire and are removed.

			Set this to a value larger than your `internal_metrics` scrape interval (default 5 minutes)
			so metrics live long enough to be emitted and captured.
			"""
		required: false
		type: float: {}
	}
	log_schema: {
		common: false
		description: """
			Default log schema for all events.

			This is used if a component does not have its own specific log schema. All events use a log
			schema, whether or not the default is used, to assign event fields on incoming events.
			"""
		required: false
		type: object: options: {
			host_key: {
				description: """
					The name of the event field to treat as the host which sent the message.

					This field will generally represent a real host, or container, that generated the message,
					but is somewhat source-dependent.
					"""
				required: false
				type: string: default: ".host"
			}
			message_key: {
				description: """
					The name of the event field to treat as the event message.

					This would be the field that holds the raw message, such as a raw log line.
					"""
				required: false
				type: string: default: ".message"
			}
			metadata_key: {
				description: """
					The name of the event field to set the event metadata in.

					Generally, this field will be set by Vector to hold event-specific metadata, such as
					annotations by the `remap` transform when an error or abort is encountered.
					"""
				required: false
				type: string: default: ".metadata"
			}
			source_type_key: {
				description: """
					The name of the event field to set the source identifier in.

					This field will be set by the Vector source that the event was created in.
					"""
				required: false
				type: string: default: ".source_type"
			}
			timestamp_key: {
				description: "The name of the event field to treat as the event timestamp."
				required:    false
				type: string: default: ".timestamp"
			}
		}
	}
	proxy: {
		common: false
		description: """
			Proxy configuration.

			Configure to proxy traffic through an HTTP(S) proxy when making external requests.

			Similar to common proxy configuration convention, you can set different proxies
			to use based on the type of traffic being proxied. You can also set specific hosts that
			should not be proxied.
			"""
		required: false
		type: object: options: {
			enabled: {
				description: "Enables proxying support."
				required:    false
				type: bool: default: true
			}
			http: {
				description: """
					Proxy endpoint to use when proxying HTTP traffic.

					Must be a valid URI string.
					"""
				required: false
				type: string: examples: ["http://foo.bar:3128"]
			}
			https: {
				description: """
					Proxy endpoint to use when proxying HTTPS traffic.

					Must be a valid URI string.
					"""
				required: false
				type: string: examples: ["http://foo.bar:3128"]
			}
			no_proxy: {
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
				type: array: {
					default: []
					items: type: string: examples: ["localhost", ".foo.bar", "*"]
				}
			}
		}
	}
	telemetry: {
		common: false
		description: """
			Telemetry options.

			Determines whether `source` and `service` tags should be emitted with the
			`component_sent_*` and `component_received_*` events.
			"""
		required: false
		type: object: options: tags: {
			description: "Configures whether to emit certain tags"
			required:    false
			type: object: options: {
				emit_service: {
					description: """
						True if the `service` tag should be emitted
						in the `component_received_*` and `component_sent_*`
						telemetry.
						"""
					required: false
					type: bool: default: false
				}
				emit_source: {
					description: """
						True if the `source` tag should be emitted
						in the `component_received_*` and `component_sent_*`
						telemetry.
						"""
					required: false
					type: bool: default: false
				}
			}
		}
	}
	timezone: {
		common: false
		description: """
			The name of the time zone to apply to timestamp conversions that do not contain an explicit time zone.

			The time zone name may be any name in the [TZ database][tzdb] or `local` to indicate system
			local time.

			Note that in Vector/VRL all timestamps are represented in UTC.

			[tzdb]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
			"""
		required: false
		type: string: examples: ["local", "America/New_York", "EST5EDT"]
	}
}
