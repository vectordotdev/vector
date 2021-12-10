package metadata

configuration: {
	configuration: #Schema
	how_it_works:  #HowItWorks
}

configuration: {
	configuration: {
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

		enrichment_tables: {
			common:      false
			description: """
				Configuration options for an [enrichment table](\(urls.enrichment_tables_concept)) to be used in a
				[`remap`](\(urls.vector_remap_transform)) transform. Currently, only [CSV](\(urls.csv)) files are
				supported.

				For the lookup in the enrichment tables to be as performant as possible, the data is indexed according
				to the fields that are used in the search. Note that indices can only be created for fields for which an
				exact match is used in the condition. For range searches, an index isn't used and the enrichment table
				drops back to a sequential scan of the data. A sequential scan shouldn't impact performance
				significantly provided that there are only a few possible rows returned by the exact matches in the
				condition. We don't recommend using a condition that uses only date range searches.
				"""
			required:    false
			type: object: options: {
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
										examples: [ ":"]
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
		}

		log_schema: {
			common: false
			description: """
				Configures default log schema for all events. This is used by
				Vector source components to assign the fields on incoming
				events.
				"""
			required: false
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
				Vector will interpolate environment variables within your configuration file
				with the following syntax:

				```toml title="vector.toml"
				[transforms.add_host]
				  type = "add_fields"

				  [transforms.add_host.fields]
				    host = "${HOSTNAME}"
				    environment = "${ENV:-development}" # default value when not present
				```
				"""

			sub_sections: [
				{
					title: "Default values"
					body: """
						Default values can be supplied via the `:-` syntax:

						```toml
						option = "${ENV_VAR:-default}"
						```
						"""
				},
				{
					title: "Escaping"
					body: """
						You can escape environment variable by preceding them with a `$` character. For
						example `$${HOSTNAME}` will be treated _literally_ in the above environment
						variable example.
						"""
				},
			]
		}
		formats: {
			title: "Formats"
			body:  """
				Vector supports [TOML](\(urls.toml)), [YAML](\(urls.yaml)), and [JSON](\(urls.json)) to
				ensure Vector fits into your workflow. A side benefit of supporting JSON is the
				enablement of data templating languages like [Jsonnet](\(urls.jsonnet)) and
				[Cue](\(urls.cue)).
				"""
		}
		location: {
			title: "Location"
			body: """
				The location of your Vector configuration file depends on your installation method. For most Linux
				based systems, the file can be found at `/etc/vector/vector.toml`.

				All files in `/etc/vector` are user configuration files and can be safely overridden to craft your
				desired Vector configuration.
				"""
		}
		multiple: {
			title: "Multiple files"
			body:  """
				You can pass multiple configuration files when starting Vector:

				```bash
				vector --config vector1.toml --config vector2.toml
				```

				Or use a [globbing syntax](\(urls.globbing)):

				```bash
				vector --config /etc/vector/*.toml
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
				type = "sink_type"
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

				```toml
				[sources.app1_logs]
				type = "file"
				includes = ["/var/log/app1.log"]

				[sources.app2_logs]
				type = "file"
				includes = ["/var/log/app.log"]

				[sources.system_logs]
				type = "file"
				includes = ["/var/log/system.log"]

				[sinks.app_logs]
				type = "datadog_logs"
				inputs = ["app*"]

				[sinks.archive]
				type = "aws_s3"
				inputs = ["*_logs"]
				```
				"""
		}
	}
}
