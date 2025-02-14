package metadata

configuration: {
	configuration: #Schema
	how_it_works:  #HowItWorks
}

configuration: {
	configuration: base.configuration.configuration

	configuration: {
		// expire_metrics's type is a little bit tricky, we could not generate `uint` from `docs::type_override` metadata macro easily.
		// So we have to define it manually, which is okay because it is already deprecated and it will be deleted soon.
		expire_metrics: {
			common: false
			description: """
				If set, Vector will configure the internal metrics system to automatically
				remove all metrics that have not been updated in the given time.

				If set to a negative value expiration is disabled.
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

		expire_metrics_secs: {
			common: false
			description: """
				Vector will expire internal metrics that haven't been emitted/updated in the
				configured interval (default 300 seconds).

				Note that internal counters that are expired but are later updated will have their
				values reset to zero. Be careful to set this value high enough to avoid expiring
				critical but infrequently updated internal counters.

				If set to a negative value expiration is disabled.
				"""
			required: false
			type: float: {
				default: 300.0
				examples: [60.0]
				unit: "seconds"
			}
		}

		expire_metrics_per_metric_set: {
			common: false
			description: """
				This allows configuring different expiration intervals for different metric sets.
				By default this is empty and any metric not matched by one of these sets will use
				the global default value, defined using `expire_metrics_secs`.
				"""
			required: false

			type: array: {
				default: []

				items: type: object: options: {
					name: {
						description: """
						Metric name to apply this expiration to. Ignores metric name if not defined.
						"""
						required: false
						type: object: options: {
							type: {
								required: true
								type: string: enum: {
									exact: "Only considers exact name matches."
									regex: "Compares metric name to the provided pattern."
								}
								description: "Metric name matcher type."
							}
							value: {
								required: true
								type: string: {}
								description: "The exact metric name."
								relevant_when: "type = \"exact\""
							}
							pattern: {
								required: true
								type: string: {}
								description: "Pattern to compare to."
								relevant_when: "type = \"regex\""
							}
						}
					}
					labels: {
						description: """
						Labels to apply this expiration to. Ignores labels if not defined.
						"""
						required: false
						type: object: options: {
							type: {
								required: true
								type: string: enum: {
									exact: "Looks for an exact match of one label key value pair."
									regex: "Compares label value with given key to the provided pattern."
									all: "Checks that all of the provided matchers can be applied to given metric."
									any: "Checks that any of the provided matchers can be applied to given metric."
								}
								description: "Metric label matcher type."
							}
							key: {
								required: true
								type: string: {}
								description: "Metric key to look for."
								relevant_when: "type = \"exact\" or type = \"regex\""
							}
							value: {
								required: true
								type: string: {}
								description: "The exact metric label value."
								relevant_when: "type = \"exact\""
							}
							pattern: {
								required: true
								type: string: {}
								description: "Pattern to compare metric label value to."
								relevant_when: "type = \"regex\""
							}
							matchers: {
								required: true
								type: array: items: type: object: {}
								description: """
								List of matchers to check. Each matcher has the same
								options as the `labels` object.
								"""
								relevant_when: "type = \"all\" or type = \"any\""
							}
						}
					}
					expire_secs: {
						common: false
						description: """
							The amount of time, in seconds, that internal metrics will persist after having not been
							updated before they expire and are removed.

							Set this to a value larger than your `internal_metrics` scrape interval (default 5 minutes)
							so that metrics live long enough to be emitted and captured.
							"""
						required: false
						type: float: {
							examples: [60.0]
							unit: "seconds"
						}
					}
				}
			}
		}

		// TODO: generate `common` and `required` fields from ruby according to some tags
		enrichment_tables: base.configuration.configuration.enrichment_tables
		enrichment_tables: common:   false
		enrichment_tables: required: false

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

							Known issues:

							- Enabling log namespacing doesn't work when disk buffers are used (see [#18574](https://github.com/vectordotdev/vector/issues/18574))
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

		// TODO: generate `common` and `required` fields from ruby according to some tags
		secret: base.configuration.configuration.secret
		secret: common:   false
		secret: required: false

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
