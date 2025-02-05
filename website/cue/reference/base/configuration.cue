package metadata

base: configuration: configuration: {
	enrichment_tables: {
		file: {
			description:   "File-specific settings."
			relevant_when: "type = \"file\""
			required:      true
			type: object: options: {
				encoding: {
					description: "File encoding configuration."
					required:    true
					type: object: options: {
						delimiter: {
							description: "The delimiter used to separate fields in each row of the CSV file."
							required:    false
							type: string: default: ","
						}
						include_headers: {
							description: """
															Whether or not the file contains column headers.

															When set to `true`, the first row of the CSV file will be read as the header row, and
															the values will be used for the names of each column. This is the default behavior.

															When set to `false`, columns are referred to by their numerical index.
															"""
							required: false
							type: bool: default: true
						}
						type: {
							description: "File encoding type."
							required:    true
							type: string: enum: csv: """
																		Decodes the file as a [CSV][csv] (comma-separated values) file.

																		[csv]: https://wikipedia.org/wiki/Comma-separated_values
																		"""
						}
					}
				}
				path: {
					description: """
						The path of the enrichment table file.

						Currently, only [CSV][csv] files are supported.

						[csv]: https://en.wikipedia.org/wiki/Comma-separated_values
						"""
					required: true
					type: string: {}
				}
			}
		}
		flush_interval: {
			description: """
				The interval used for making writes visible in the table.
				Longer intervals might get better performance,
				but there is a longer delay before the data is visible in the table.
				Since every TTL scan makes its changes visible, only use this value
				if it is shorter than the `scan_interval`.

				By default, all writes are made visible immediately.
				"""
			relevant_when: "type = \"memory\""
			required:      false
			type: uint: {}
		}
		internal_metrics: {
			description:   "Configuration of internal metrics"
			relevant_when: "type = \"memory\""
			required:      false
			type: object: options: include_key_tag: {
				description: """
					Determines whether to include the key tag on internal metrics.

					This is useful for distinguishing between different keys while monitoring. However, the tag's
					cardinality is unbounded.
					"""
				required: false
				type: bool: default: false
			}
		}
		locale: {
			description: """
				The locale to use when querying the database.

				MaxMind includes localized versions of some of the fields within their database, such as
				country name. This setting can control which of those localized versions are returned by the
				transform.

				More information on which portions of the geolocation data are localized, and what languages
				are available, can be found [here][locale_docs].

				[locale_docs]: https://support.maxmind.com/hc/en-us/articles/4414877149467-IP-Geolocation-Data#h_01FRRGRYTGZB29ERDBZCX3MR8Q
				"""
			relevant_when: "type = \"geoip\""
			required:      false
			type: string: default: "en"
		}
		max_byte_size: {
			description: """
				Maximum size of the table in bytes. All insertions that make
				this table bigger than the maximum size are rejected.

				By default, there is no size limit.
				"""
			relevant_when: "type = \"memory\""
			required:      false
			type: uint: {}
		}
		path: {
			description: """
				Path to the [MaxMind GeoIP2][geoip2] or [GeoLite2 binary city database file][geolite2]
				(**GeoLite2-City.mmdb**).

				Other databases, such as the country database, are not supported.
				`mmdb` enrichment table can be used for other databases.

				[geoip2]: https://dev.maxmind.com/geoip/geoip2/downloadable
				[geolite2]: https://dev.maxmind.com/geoip/geoip2/geolite2/#Download_Access
				"""
			relevant_when: "type = \"geoip\" or type = \"mmdb\""
			required:      true
			type: string: {}
		}
		scan_interval: {
			description: """
				The scan interval used to look for expired records. This is provided
				as an optimization to ensure that TTL is updated, but without doing
				too many cache scans.
				"""
			relevant_when: "type = \"memory\""
			required:      false
			type: uint: default: 30
		}
		schema: {
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
			relevant_when: "type = \"file\""
			required:      false
			type: object: options: "*": {
				description: "Represents mapped log field names and types."
				required:    true
				type: string: {}
			}
		}
		ttl: {
			description: """
				TTL (time-to-live in seconds) is used to limit the lifetime of data stored in the cache.
				When TTL expires, data behind a specific key in the cache is removed.
				TTL is reset when the key is replaced.
				"""
			relevant_when: "type = \"memory\""
			required:      false
			type: uint: default: 600
		}
		type: {
			description: "enrichment table type"
			required:    true
			type: string: enum: {
				file: "Exposes data from a static file as an enrichment table."
				geoip: """
					Exposes data from a [MaxMind][maxmind] [GeoIP2][geoip2] database as an enrichment table.

					[maxmind]: https://www.maxmind.com/
					[geoip2]: https://www.maxmind.com/en/geoip2-databases
					"""
				memory: """
					Exposes data from a memory cache as an enrichment table. The cache can be written to using
					a sink.
					"""
				mmdb: """
					Exposes data from a [MaxMind][maxmind] database as an enrichment table.

					[maxmind]: https://www.maxmind.com/
					"""
			}
		}
	}
	secrets: {
		auth: {
			description:   "Configuration of the authentication strategy for interacting with AWS services."
			relevant_when: "type = \"aws_secrets_manager\""
			required:      false
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
		command: {
			description: """
				Command arguments to execute.

				The path to the script or binary must be the first argument.
				"""
			relevant_when: "type = \"exec\""
			required:      true
			type: array: items: type: string: {}
		}
		endpoint: {
			description:   "Custom endpoint for use with AWS-compatible services."
			relevant_when: "type = \"aws_secrets_manager\""
			required:      false
			type: string: examples: ["http://127.0.0.0:5000/path/to/service"]
		}
		path: {
			description:   "File path to read secrets from."
			relevant_when: "type = \"file\" or type = \"directory\""
			required:      true
			type: string: {}
		}
		region: {
			description: """
				The [AWS region][aws_region] of the target service.

				[aws_region]: https://docs.aws.amazon.com/general/latest/gr/rande.html#regional-endpoints
				"""
			relevant_when: "type = \"aws_secrets_manager\""
			required:      false
			type: string: examples: [
				"us-east-1",
			]
		}
		remove_trailing_whitespace: {
			description:   "Remove trailing whitespace from file contents."
			relevant_when: "type = \"directory\""
			required:      false
			type: bool: default: false
		}
		secret_id: {
			description:   "ID of the secret to resolve."
			relevant_when: "type = \"aws_secrets_manager\""
			required:      true
			type: string: {}
		}
		timeout: {
			description:   "The timeout, in seconds, to wait for the command to complete."
			relevant_when: "type = \"exec\""
			required:      false
			type: uint: default: 5
		}
		tls: {
			description:   "TLS configuration."
			relevant_when: "type = \"aws_secrets_manager\""
			required:      false
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
		type: {
			description: "secret type"
			required:    true
			type: string: enum: {
				aws_secrets_manager: "AWS Secrets Manager."
				directory:           "Directory."
				exec:                "Exec."
				file:                "File."
			}
		}
	}
}
