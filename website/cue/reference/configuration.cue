package metadata

configuration: {
	configuration: #Schema
	how_it_works:  #HowItWorks
}

configuration: {
	configuration: {
		acknowledgements: {
			common:      true
			description: "Controls how acknowledgements are handled by all sources. These settings may be overridden in individual sources."
			required:    false
			type: object: options: {
				enabled: {
					common:      true
					description: "Controls if sources will wait for destination sinks to deliver the events, or persist them to a disk buffer, before acknowledging receipt. If set to `true`, all capable sources will have acknowledgements enabled."
					warnings: ["Disabling this option may lead to loss of data, as destination sinks may reject events after the source acknowledges their successful receipt."]
					required: false
					type: bool: default: false
				}
			}
		}

		data_dir: {
			common: false
			description: """
				The directory used for persisting Vector state, such
				as on-disk buffers, file checkpoints, and more.
				Please make sure the Vector project has write
				permissions to this directory.
				"""
			required: false
			type: string: {
				default: "/var/lib/vector/"
				examples: ["/var/lib/vector", "/var/local/lib/vector/", "/home/user/vector/"]
			}
		}

		expire_metrics: {
			common: false
			description: """
				If set, Vector will configure the internal metrics system to automatically
				remove all metrics that have not been updated in the given time.
				This value must be positive.
				"""
			required: false
			warnings: ["Deprecated, please use `expire_metrics_secs` instead."]
			type: object: options: {
				secs: {
					common:      true
					required:    false
					description: "The whole number of seconds after which to expire metrics."
					type: uint: {
						default: null
						examples: [60]
						unit: "seconds"
					}
				}
				nsecs: {
					common:      true
					required:    false
					description: "The fractional number of seconds after which to expire metrics."
					type: uint: {
						default: null
						examples: [0]
						unit: "nanoseconds"
					}
				}
			}
		}

		expire_metrics_secs: {
			common: false
			description: """
				Vector will expire internal metrics that haven't been emitted/updated in the
				configured interval (default 300 seconds). This value must be positive.

				Note that internal counters that are expired but are later updated will have their
				values reset to zero. Be careful to set this value high enough to avoid expiring
				critical but infrequently updated internal counters.
				"""
			required: false
			type: float: {
				default: 300.0
				examples: [60.0]
				unit: "seconds"
			}
		}

		enrichment_tables: {
			common:      false
			description: """
				Configuration options for an [enrichment table](\(urls.enrichment_tables_concept)) to be used in a
				[`remap`](\(urls.vector_remap_transform)) transform. Currently supported are:

				* [CSV](\(urls.csv)) files
				* [MaxMind](\(urls.maxmind)) databases

				For the lookup in the enrichment tables to be as performant as possible, the data is indexed according
				to the fields that are used in the search. Note that indices can only be created for fields for which an
				exact match is used in the condition. For range searches, an index isn't used and the enrichment table
				drops back to a sequential scan of the data. A sequential scan shouldn't impact performance
				significantly provided that there are only a few possible rows returned by the exact matches in the
				condition. We don't recommend using a condition that uses only date range searches.
				"""
			required:    false
			type: object: options: {
				type: {
					description: """
						Determines the type of enrichment data that is to be loaded.
						"""
					required: true
					type: string: {
						enum: {
							"file":  "Enrich data from a CSV file."
							"geoip": "Enrich data from a [GeoIp](\(urls.maxmind_geoip2)) [MaxMind](\(urls.maxmind)) database."
							"mmdb":  "Enrich data from any [MaxMind](\(urls.maxmind)) database."
						}
					}
				}
				file: {
					required:    true
					description: "Configuration options for the file that provides the enrichment table."
					type: object: options: {
						path: {
							description: """
								The path of the enrichment table file. Currently, only [CSV](\(urls.csv)) files are
								supported.
								"""
							warnings: [
								"In order to be used by Vector, you need to assign read access to the enrichment table file.",
							]
							required: true
							type: string: {
								examples: [
									"/data/info.csv",
									"./info.csv",
								]
							}
						}

						encoding: {
							description: "Configuration options for the encoding of the enrichment table's file."
							required:    true
							type: object: options: {
								type: {
									description: """
										The encoding of the file. Currently, only [CSV](\(urls.csv)) is supported.
										"""
									required:    false
									common:      true
									type: string: default: "csv"
								}

								delimiter: {
									description: "The delimiter used to separate fields in each row of the CSV file."
									common:      false
									required:    false
									type: string: {
										default: ","
										examples: [":"]
									}
								}

								include_headers: {
									description: """
										Set `include_headers` to `true` if the first row of the CSV file contains the
										headers for each column. This is the default behavior.

										If you set it to `false`, there are no headers and the columns are referred to
										by their numerical index.
										"""
									required: false
									common:   false
									type: bool: default: true
								}
							}
						}

						schema: {
							description: _coercing_fields
							required:    false
							common:      true
							type: object: {
								examples: [
									{
										status:            "int"
										duration:          "float"
										success:           "bool"
										timestamp_iso8601: "timestamp|%F"
										timestamp_custom:  "timestamp|%a %b %e %T %Y"
										timestamp_unix:    "timestamp|%F %T"
									},
								]

								options: {}
							}
						}
					}
				}
			}
			type: object: options: {
				geoip: {
					required:    true
					description: """
						Configuration options for [MaxMind](\(urls.maxmind)) databases.

						The following [MaxMind](\(urls.maxmind)) databases are currently supported:

						* [GeoLite2-ASN.mmdb](\(urls.maxmind_geolite2_asn)) (free) — Determine the
							autonomous system number and organization associated with an IP address.
						* [GeoLite2-City.mmdb](\(urls.maxmind_geolite2_city)) (free) — Determine the
							country, subdivisions, city, and postal code associated with IPv4 and IPv6
							addresses worldwide.
						* [GeoIP2-City.mmdb](\(urls.maxmind_geoip2_city)) (paid) — Determine the country,
							subdivisions, city, and postal code associated with IPv4 and IPv6
							addresses worldwide.
						* [GeoIP2-ISP.mmdb](\(urls.maxmind_geoip2_isp)) (paid) — Determine the Internet
							Service Provider (ISP), organization name, and autonomous system organization
							and number associated with an IP address.
						* [GeoIP2-Anonymous-IP.mmdb](\(urls.maxmind_geoip2_anonymous_ip)) (paid) — Determine
							proxy, VPN, hosting, and other anonymous IP addresses.	

						The database file should be in the [MaxMind DB file format](\(urls.maxmind_db_file_format)).

						This enrichment table only supports lookup with IP address.
						"""
					type: object: options: {
						path: {
							description: """
								Path to the database file.
								"""
							required: true
							type: string: {
								examples: ["/path/to/GeoLite2-City.mmdb", "/path/to/GeoLite2-ISP.mmdb"]
							}
						}
						locale: {
							description: """
								The locale to use to lookup the country name and region name for the city database.
								See [Locations Files](https://dev.maxmind.com/geoip/docs/databases/city-and-country?lang=en)
								"""
							required: false
							common:   false
							type: string: {
								default: "en"
								examples: ["de", "en", "es", "fr", "ja", "pt-BR", "ru", "zh-CN"]
							}
						}
					}
				}
			}
			type: object: options: {
				mmdb: {
					required:    true
					description: """
						Configuration options for generic [MaxMind](\(urls.maxmind)) databases.

						The database file should be in the [MaxMind DB file format](\(urls.maxmind_db_file_format)).

						This enrichment table only supports lookup with IP address.
						"""
					type: object: options: {
						path: {
							description: """
								Path to the database file.
								"""
							required: true
							type: string: {
								examples: ["/path/to/GeoLite2-City.mmdb", "/path/to/GeoLite2-ISP.mmdb"]
							}
						}
					}
				}
			}
		}
		schema: {
			common: false
			description: """
				Configures options for how Vector handles event schema.
				"""
			required: false
			type: object: {
				examples: []
				options: {
					log_namespace: {
						common:      false
						description: """
							Globally enables / disables log namespacing. See [Log Namespacing](\(urls.log_namespacing_blog))
							for more details. If you want to enable individual sources, there is a config
							option in the source configuration.
							"""
						required:    false
						warnings: []
						type: bool: default: false
					}
				}
			}
		}

		telemetry: {
			common: false
			description: """
				Configures options for how Vector emits telemetry.
				"""
			required: false
			type: object: {
				examples: []
				options: {
					tags: {
						required: false
						description: """
							Controls which tags should be included with the `vector_component_sent_events_total` and
							`vector_component_sent_event_bytes_total` metrics.
							"""
						type: object: {
							examples: []
							options: {
								emit_source: {
									common: true
									description: """
										Add a `source` tag with the source component the event was received from.

										If there is no source component, for example if the event was generated by
										the `lua` transform a `-` is emitted for this tag.
										"""
									required: false
									type: bool: {
										default: false
									}
								}
								emit_service: {
									common: false
									description: """
										Adds a `service` tag with the service component the event was received from.

										For logs this is the field that has been determined to mean `service`. Each source may
										define different fields for this. For example, with `syslog` events the `appname` field
										is used.

										Metric events will use the tag named `service`.

										If no service is available a `-` is emitted for this tag.
										"""
									required: false
									type: bool: {
										default: false
									}
								}
							}
						}
					}
				}
			}
		}

		log_schema: {
			common:      false
			description: """
				Configures default log schema for all events. This is used by
				Vector components to assign the fields on incoming
				events.
				These values are ignored if log namespacing is enabled. (See [Log Namespacing](\(urls.log_namespacing_blog)))
				"""
			required:    false
			type: object: {
				examples: []
				options: {
					message_key: {
						common: true
						description: """
							Sets the event key to use for the event message field.
							"""
						required: false
						type: string: {
							default: "message"
							examples: ["message", "@message"]
						}
					}

					timestamp_key: {
						common: true
						description: """
							Sets the event key to use for the event timestamp field.
							"""
						required: false
						type: string: {
							default: "timestamp"
							examples: ["timestamp", "@timestamp"]
						}
					}

					host_key: {
						common: true
						description: """
							Sets the event key to use for the event host field.
							"""
						required: false
						type: string: {
							default: "host"
							examples: ["host", "@host"]
						}
					}

					source_type_key: {
						common: true
						description: """
							Sets the event key to use for the event source type
							field that is set by some sources.
							"""
						required: false
						type: string: {
							default: "source_type"
							examples: ["source_type", "@source_type"]
						}
					}

					metadata_key: {
						common: true
						description: """
							Sets the event key to use for event metadata field (e.g. error or
							abort annotations in the `remap` transform).
							"""
						required: false
						type: string: {
							default: "metadata"
							examples: ["@metadata", "meta"]
							syntax: "literal"
						}
					}
				}
			}
		}

		healthchecks: {
			common: false
			description: """
				Configures health checks for all sinks.
				"""
			required: false
			type: object: {
				examples: []
				options: {
					enabled: {
						common: true
						description: """
							Disables all health checks if false, otherwise sink specific
							option overrides it.
							"""
						required: false
						type: bool: {
							default: true
						}
					}

					require_healthy: {
						common: false
						description: """
							Exit on startup if any sinks' health check fails. Overridden by
							`--require-healthy` command line flag.
							"""
						required: false
						type: bool: {
							default: false
						}
					}
				}
			}
		}

		secret: {
			common: false
			description: """
				Configuration options to retrieve secrets from external backend in order to avoid storing secrets in plaintext
				in Vector config. Multiple backends can be configured. Use `SECRET[<backend_name>.<secret_key>]` to tell Vector to retrieve the secret. This placeholder is replaced by the secret
				retrieved from the relevant backend.
				"""
			required: false
			type: object: options: {
				exec: {
					required: true
					description: """
						Run a local command to retrieve secrets.

						The provided command will be run and provided a list of
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

						Secrets are loaded when Vector starts or if Vector receives a `SIGHUP` signal triggering its
						configuration reload process.
						"""
					type: object: options: {
						command: {
							description: """
								The command to be run, plus any arguments required.
								"""
							required: true
							type: array: {
								examples: [["/path/to/get-secret", "-s"], ["/path/to/vault-wrapper"]]
								items: type: string: {}
							}
						}
						timeout: {
							description: "The amount of time Vector will wait for the command to complete."
							required:    false
							common:      false
							type: uint: {
								default: 5
								unit:    "seconds"
							}
						}
					}
				}
				aws_secrets_manager: {
					required: true
					description: """
						Retrieve secrets from AWS Secrets Manager.

						The secret must be a JSON text string with key/value pairs. For example:
						```json
						{
							"username": "test",
							"password": "example-password"
						}
						```

						If an error occurred retrieving the secrets, Vector logs the error and exits.

						Secrets are loaded when Vector starts or if Vector receives a `SIGHUP` signal triggering its
						configuration reload process.
						"""
					type: object: options: {
						secret_id: {
							description: """
								The ID of the secret to be retrieved.
								"""
							required: true
							type: string: {
								examples: ["/secret/foo-bar"]
							}
						}
					}
				}
			}
		}

		timezone: {
			common:      false
			description: """
				The name of the time zone to apply to timestamp conversions that do not contain an
				explicit time zone. The time zone name may be any name in the
				[TZ database](\(urls.tz_time_zones)), or `local` to indicate system local time.
				"""
			required:    false
			type: string: {
				default: "local"
				examples: ["local", "America/NewYork", "EST5EDT"]
			}
		}

		proxy: {
			common:      false
			description: "Configures an HTTP(S) proxy for Vector to use."
			required:    false
			type: object: options: {
				enabled: {
					common:      false
					description: "If false the proxy will be disabled."
					required:    false
					type: bool: default: true
				}
				http: {
					common:      false
					description: "The URL to proxy HTTP requests through."
					required:    false
					type: string: {
						default: null
						examples: ["http://foo.bar:3128"]
					}
				}
				https: {
					common:      false
					description: "The URL to proxy HTTPS requests through."
					required:    false
					type: string: {
						default: null
						examples: ["http://foo.bar:3128"]
					}
				}
				no_proxy: {
					common:      false
					description: """
							A list of hosts to avoid proxying. Allowed patterns here include:

							Pattern | Example match
							:-------|:-------------
							Domain names | `example.com` matches requests to `example.com`
							Wildcard domains | `.example.com` matches requests to `example.com` and its subdomains
							IP addresses | `127.0.0.1` matches requests to 127.0.0.1
							[CIDR](\(urls.cidr)) blocks | `192.168.0.0./16` matches requests to any IP addresses in this range
							Splat | `*` matches all hosts
							"""
					required:    false

					type: array: {
						default: null
						items: type: string: {
							examples: ["localhost", ".foo.bar", "*"]
						}
					}
				}
			}
		}
	}

	how_it_works: {
		environment_variables: {
			title: "Environment variables"
			body: """
				Vector interpolates environment variables within your configuration file
				with the following syntax:

				```toml title="vector.yaml"
				transforms:
					add_host:
						inputs: ["apache_logs"]
						type: "remap"
						source: |
							.host = get_env_var!("HOSTNAME")
				```
				"""

			sub_sections: [
				{
					title: "Default values"
					body: """
						Default values can be supplied using `:-` or `-` syntax:

						```toml
						option = "${ENV_VAR:-default}" # default value if variable is unset or empty
						option = "${ENV_VAR-default}" # default value only if variable is unset
						```
						"""
				},
				{
					title: "Required variables"
					body: """
						Environment variables that are required can be specified using `:?` or `?` syntax:

						```toml
						option = "${ENV_VAR:?err}" # Vector exits with 'err' message if variable is unset or empty
						option = "${ENV_VAR?err}" # Vector exits with 'err' message only if variable is unset
						```
						"""
				},
				{
					title: "Escaping"
					body: """
						You can escape environment variable by preceding them with a `$` character. For
						example `$${HOSTNAME}` or `$$HOSTNAME` is treated literally in the above environment
						variable example.
						"""
				},
			]
		}
		secrets_management: {
			title: "Secrets management"
			body: """
				Vector can retrieve secrets like a password or token by querying an external system in order to
				avoid storing sensitive information in Vector configuration files. Secret backends used to retrieve
				sensitive token are configured in a dedicated section (`secret`). In the rest of the configuration you should use
				the `SECRET[<backend_name>.<secret_key>]` notation to interpolate the secret. Interpolation will happen immediately after
				environment variables interpolation. While Vector supports multiple commands to retrieve secrets, a
				secret backend cannot use the secret interpolation feature for its own configuration. Currently the only supported
				kind of secret backend is the `exec` one that runs an external command to retrieve secrets.

				The following example shows a simple configuration with two backends defined:

				```toml title="vector.yaml"
				secret:
					backend_1:
						type: "exec"
						command: ["/path/to/cmd1", "--some-option"]
					backend_2:
						type: "exec"
						command: ["/path/to/cmd2"]

				sinks:
					dd_logs:
						type: "datadog_logs"
						default_api_key: "SECRET[backend_1.dd_api_key]"

					splunk:
						type: "splunk_hec"
						default_token: "SECRET[backend_2.splunk_token]"
				```

				In that example Vector will retrieve the `dd_api_key` from `backen_1` and `splunk_token` from `backend_2`.
				"""

			sub_sections: [
				{
					title: "The `exec` backend"
					body:  """
						When using the `exec` type for a secret backend Vector and the external command are communicating using
						the standard input and output. The communication is using plain text JSON. Vector spawns the specified
						command, writes the secret retrieval query using JSON on the standard input of the child process and
						reads its standard output expecting a JSON object. Standard error from the child process is read and
						logged by Vector and can be used for troubleshooting.

						The JSON object used for communication between an `exec` backend and Vector shall comply with the
						[Datadog Agent executable API](\(urls.datadog_agent_exec_api)).

						For example a query would look like:

						```json
						{
							"version": "1.0",
							"secrets": ["dd_api_key", "another_dd_api_key"]
						}
						```

						A possible reply from a backend could then be:

						```json
						{
							"dd_api_key": {"value": "A_DATADOG_API_KEY", "error": null},
							"another_dd_api_key": {"value": null, "error": "unable to retrieve secret"}
						}
						```

						If a backend writes a JSON that does not follow the expected structure or reports an error for a
						given secret Vector will refuse to load the configuration.

						Currently Vector will always query backend with `"version": "1.0"`.
						"""
				},
			]
		}
		formats: {
			title: "Formats"
			body:  """
				Vector supports [YAML](\(urls.yaml)), [TOML](\(urls.toml)), and [JSON](\(urls.json)) to
				ensure Vector fits into your workflow. A side benefit of supporting YAML and JSON is that they
				enable you to use data templating languages such as [ytt](\(urls.ytt)), [Jsonnet](\(urls.jsonnet)) and
				[Cue](\(urls.cue)).
				"""
		}
		location: {
			title: "Location"
			body: """
				The location of your Vector configuration file depends on your installation method. For most Linux
				based systems, the file can be found at `/etc/vector/vector.yaml`.

				All files in `/etc/vector` are user configuration files and can be safely overridden to craft your
				desired Vector configuration.
				"""
		}
		multiple: {
			title: "Multiple files"
			body:  """
				You can pass multiple configuration files when starting Vector:

				```bash
				vector --config vector1.yaml --config vector2.yaml
				```

				Or use a [globbing syntax](\(urls.globbing)):

				```bash
				vector --config /etc/vector/*.yaml
				```
				"""
		}
		automatic_namespacing: {
			title: "Automatic namespacing of component files"
			body: """
				You can split your configuration files in component-type related folders.

				For example, you can create the sink `foo` in the folder `/path/to/vector/config/sinks/foo.toml` and
				configure it as follows:

				```toml
				type: "sink_type"
				# here the sinks options
				```

				You can do the same for other kinds of components like `sources`, `transforms`, `tests` and `enrichment_tables`.

				For vector to find and load the different configuration files, you need to load your configuration with
				the `--config-dir` argument.

				```bash
				vector --config-dir /path/to/vector/config
				```
				"""
		}
		wildcards: {
			title: "Wildcards in component names"
			body: """
				Vector supports wildcard characters (`*`) in component names when building your topology.

				For example:

				```yaml
				sources:
					app1_logs:
						type: "file"
						includes: ["/var/log/app1.log"]

					app2_logs:
						type: "file"
						includes: ["/var/log/app.log"]

					system_logs:
						type: "file"
						includes: ["/var/log/system.log"]

				sinks:
					app_logs:
						type: "datadog_logs"
						inputs: ["app*"]

					archive:
						type: "aws_s3"
						inputs: ["*_logs"]
				```
				"""
		}
	}
}
