package metadata

components: {
	{[Kind=string]: [Name=string]: {
		kind: string
		let Kind = kind

		configuration: {
			_conditions: {
				examples: [
					{
						type:                           "check_fields"
						"message.eq":                   "foo"
						"message.not_eq":               "foo"
						"message.exists":               true
						"message.not_exists":           true
						"message.contains":             "foo"
						"message.not_contains":         "foo"
						"message.ends_with":            "foo"
						"message.not_ends_with":        "foo"
						"message.ip_cidr_contains":     "10.0.0.0/8"
						"message.not_ip_cidr_contains": "10.0.0.0/8"
						"message.regex":                " (any|of|these|five|words) "
						"message.not_regex":            " (any|of|these|five|words) "
						"message.starts_with":          "foo"
						"message.not_starts_with":      "foo"
					},
				]
				options: {
					type: {
						common:      true
						description: "The type of the condition to execute."
						required:    false
						warnings: []
						type: string: {
							default: "check_fields"
							enum: {
								check_fields: "Allows you to check individual fields against a list of conditions."
								is_log:       "Returns true if the event is a log."
								is_metric:    "Returns true if the event is a metric."
							}
						}
					}
					"*.eq": {
						common:      true
						description: "Check whether a field's contents exactly matches the value specified, case sensitive. This may be a single string or a list of strings, in which case this evaluates to true if any of the list matches."
						required:    false
						warnings: []
						type: string: {
							default: null
							examples: ["foo"]
						}
					}
					"*.exists": {
						common:      false
						description: "Check whether a field exists or does not exist, depending on the provided value being `true` or `false` respectively."
						required:    false
						warnings: []
						type: bool: default: null
					}
					"*.not_*": {
						common:      false
						description: "Allow you to negate any condition listed here."
						required:    false
						warnings: []
						type: string: {
							default: null
							examples: []
						}
					}
					"*.contains": {
						common:      true
						description: "Checks whether a string field contains a string argument, case sensitive. This may be a single string or a list of strings, in which case this evaluates to true if any of the list matches."
						required:    false
						warnings: []
						type: string: {
							default: null
							examples: ["foo"]
						}
					}
					"*.ends_with": {
						common:      true
						description: "Checks whether a string field ends with a string argument, case sensitive. This may be a single string or a list of strings, in which case this evaluates to true if any of the list matches."
						required:    false
						warnings: []
						type: string: {
							default: null
							examples: ["suffix"]
						}
					}
					"*.ip_cidr_contains": {
						common:      false
						description: "Checks whether an IP field is contained within a given [IP CIDR](\(urls.cidr)) (works with IPv4 and IPv6). This may be a single string or a list of strings, in which case this evaluates to true if the IP field is contained within any of the CIDRs in the list."
						required:    false
						warnings: []
						type: string: {
							default: null
							examples: ["10.0.0.0/8", "2000::/10", "192.168.0.0/16"]
						}
					}
					"*.regex": {
						common:      true
						description: "Checks whether a string field matches a [regular expression](\(urls.regex)). Vector uses the [documented Rust Regex syntax](\(urls.rust_regex_syntax)). Note that this condition is considerably more expensive than a regular string match (such as `starts_with` or `contains`) so the use of those conditions are preferred where possible."
						required:    false
						warnings: []
						type: string: {
							default: null
							examples: [" (any|of|these|five|words) "]
						}
					}
					"*.starts_with": {
						common:      true
						description: "Checks whether a string field starts with a string argument, case sensitive. This may be a single string or a list of strings, in which case this evaluates to true if any of the list matches."
						required:    false
						warnings: []
						type: string: {
							default: null
							examples: ["prefix"]
						}
					}
				}
			}

			_tls_accept: {
				_args: {
					can_enable:             bool
					can_verify_certificate: bool | *true
					enabled_default:        bool
				}
				let Args = _args

				common:      false
				description: "Configures the TLS options for incoming connections."
				required:    false
				type: object: options: {
					if Args.can_enable {
						enabled: {
							common:      false
							description: "Require TLS for incoming connections. If this is set, an identity certificate is also required."
							required:    false
							type: bool: default: Args.enabled_default
						}
					}

					ca_file: {
						common:      false
						description: "Absolute path to an additional CA certificate file, in DER or PEM format (X.509), or an in-line CA certificate in PEM format."
						required:    false
						type: string: {
							default: null
							examples: ["/path/to/certificate_authority.crt"]
						}
					}
					crt_file: {
						common:      false
						description: "Absolute path to a certificate file used to identify this server, in DER or PEM format (X.509) or PKCS#12, or an in-line certificate in PEM format. If this is set, and is not a PKCS#12 archive, `key_file` must also be set. This is required if `enabled` is set to `true`."
						required:    false
						type: string: {
							default: null
							examples: ["/path/to/host_certificate.crt"]
						}
					}
					key_file: {
						common:      false
						description: "Absolute path to a private key file used to identify this server, in DER or PEM format (PKCS#8), or an in-line private key in PEM format."
						required:    false
						type: string: {
							default: null
							examples: ["/path/to/host_certificate.key"]
						}
					}
					key_pass: {
						common:      false
						description: "Pass phrase used to unlock the encrypted key file. This has no effect unless `key_file` is set."
						required:    false
						type: string: {
							default: null
							examples: ["${KEY_PASS_ENV_VAR}", "PassWord1"]
						}
					}

					if Args.can_verify_certificate {
						verify_certificate: {
							common:      false
							description: "If `true`, Vector will require a TLS certificate from the connecting host and terminate the connection if the certificate is not valid. If `false` (the default), Vector will not request a certificate from the client."
							required:    false
							type: bool: default: false
						}
					}
				}
			}

			_tls_connect: {
				_args: {
					can_enable:             bool
					can_verify_certificate: bool | *true
					can_verify_hostname:    bool | *false
					enabled_default:        bool
				}
				let Args = _args

				common:      false
				description: "Configures the TLS options for incoming connections."
				required:    false
				type: object: options: {
					if Args.can_enable {
						enabled: {
							common:      true
							description: "Enable TLS during connections to the remote."
							required:    false
							type: bool: default: Args.enabled_default
						}
					}

					ca_file: {
						common:      false
						description: "Absolute path to an additional CA certificate file, in DER or PEM format (X.509), or an inline CA certificate in PEM format."
						required:    false
						type: string: {
							default: null
							examples: ["/path/to/certificate_authority.crt"]
						}
					}
					crt_file: {
						common:      true
						description: "Absolute path to a certificate file used to identify this connection, in DER or PEM format (X.509) or PKCS#12, or an inline certificate in PEM format. If this is set and is not a PKCS#12 archive, `key_file` must also be set."
						required:    false
						type: string: {
							default: null
							examples: ["/path/to/host_certificate.crt"]
						}
					}
					key_file: {
						common:      true
						description: "Absolute path to a private key file used to identify this connection, in DER or PEM format (PKCS#8), or an inline private key in PEM format. If this is set, `crt_file` must also be set."
						required:    false
						type: string: {
							default: null
							examples: ["/path/to/host_certificate.key"]
						}
					}
					key_pass: {
						common:      false
						description: "Pass phrase used to unlock the encrypted key file. This has no effect unless `key_file` is set."
						required:    false
						type: string: {
							default: null
							examples: ["${KEY_PASS_ENV_VAR}", "PassWord1"]
						}
					}

					if Args.can_verify_certificate {
						verify_certificate: {
							common:      false
							description: "If `true` (the default), Vector will validate the TLS certificate of the remote host."
							required:    false
							type: bool: default: true
						}
					}

					if Args.can_verify_hostname {
						verify_hostname: {
							common:      false
							description: "If `true` (the default), Vector will validate the configured remote host name against the remote host's TLS certificate. Do NOT set this to `false` unless you understand the risks of not verifying the remote hostname."
							required:    false
							type: bool: default: true
						}
					}
				}
			}

			_http_auth: {
				_args: {
					password_example: string
					username_example: string
				}
				let Args = _args

				common:      false
				description: "Configures the authentication strategy."
				required:    false
				type: object: options: {
					password: {
						description: "The basic authentication password."
						required:    true
						warnings: []
						type: string: {
							examples: [Args.password_example, "password"]
						}
					}
					strategy: {
						description: "The authentication strategy to use."
						required:    true
						warnings: []
						type: string: {
							enum: {
								basic:  "The [basic authentication strategy](\(urls.basic_auth))."
								bearer: "The bearer token authentication strategy."
							}
						}
					}
					token: {
						description: "The token to use for bearer authentication"
						required:    true
						warnings: []
						type: string: {
							examples: ["${API_TOKEN}", "xyz123"]
						}
					}
					user: {
						description: "The basic authentication user name."
						required:    true
						warnings: []
						type: string: {
							examples: [Args.username_example, "username"]
						}
					}
				}
			}

			_types: {
				common:      true
				description: "Key/value pairs representing mapped log field names and types. This is used to coerce log fields into their proper types."
				required:    false
				warnings: []
				type: object: {
					examples: [
						{
							status:            "int"
							duration:          "float"
							success:           "bool"
							timestamp_iso8601: "timestamp|%F"
							timestamp_custom:  "timestamp|%a %b %e %T %Y"
							parent: {"child": "int"}
						},
					]
					options: {}
				}
			}

			if Kind != "source" {
				inputs: {
					description: "A list of upstream [source](\(urls.vector_sources)) or [transform](\(urls.vector_transforms)) IDs. See [configuration](\(urls.vector_configuration)) for more info."
					required:    true
					sort:        -1
					type: array: items: type: string: examples: ["my-source-or-transform-id"]
				}
			}

			"type": {
				description: "The component type. This is a required field for all components and tells Vector which component to use."
				required:    true
				sort:        -2
				"type": string: enum:
					"\(Name)": "The type of this component."
			}
		}

		features: {
			descriptions: {
				if features.buffer != _|_ {
					if features.buffer.enabled == true {
						buffer: "Buffers data in-memory or on-disk for performance and durability."
					}
				}

				if features.collect != _|_ {
					if features.collect.from != _|_ {
						collect_context: "Enriches data with useful \(features.collect.from.name) context."
					}

					if features.collect.checkpoint.enabled != _|_ {
						checkpoint: "Efficiently collects data and checkpoints read positions to ensure data is not lost between restarts."
					}

					if features.collect.tls.enabled != _|_ {
						tls_collect: "Securely collects data via Transport Layer Security (TLS)."
					}
				}

				if features.multiline != _|_ {
					if features.multiline.enabled == true {
						multiline: "Merges multi-line logs into one event."
					}
				}

				if features.receive != _|_ {
					if features.receive.from != _|_ {
						receive_context: "Enriches data with useful \(features.receive.from.name) context."
					}

					if features.receive.tls.enabled != _|_ {
						tls_receive: "Securely receives data via Transport Layer Security (TLS)."
					}
				}

				if features.send != _|_ {
					if features.send.batch != _|_ {
						if features.send.batch.enabled {
							batch: "Batches data to maximize throughput."
						}
					}

					if features.send.compression.enabled != _|_ {
						compress: "Compresses data to optimize bandwidth."
					}

					if features.send.request.enabled != _|_ {
						request: "Automatically retries failed requests, with backoff."
					}

					if features.send.tls.enabled != _|_ {
						tls_send: "Securely transmits data via Transport Layer Security (TLS)."
					}
				}
			}
		}

		if Kind == "source" || Kind == "transform" {
			output: {
				_passthrough_counter: {
					description: data_model.schema.metric.type.object.options.counter.description
					tags: {
						"*": {
							description: "Any tags present on the metric."
							examples: [_values.local_host]
							required: false
						}
					}
					type: "counter"
				}

				_passthrough_distribution: {
					description: data_model.schema.metric.type.object.options.distribution.description
					tags: {
						"*": {
							description: "Any tags present on the metric."
							examples: [_values.local_host]
							required: false
						}
					}
					type: "distribution"
				}

				_passthrough_gauge: {
					description: data_model.schema.metric.type.object.options.gauge.description
					tags: {
						"*": {
							description: "Any tags present on the metric."
							examples: [_values.local_host]
							required: false
						}
					}
					type: "gauge"
				}

				_passthrough_histogram: {
					description: data_model.schema.metric.type.object.options.histogram.description
					tags: {
						"*": {
							description: "Any tags present on the metric."
							examples: [_values.local_host]
							required: false
						}
					}
					type: "gauge"
				}

				_passthrough_set: {
					description: data_model.schema.metric.type.object.options.set.description
					tags: {
						"*": {
							description: "Any tags present on the metric."
							examples: [_values.local_host]
							required: false
						}
					}
					type: "gauge"
				}

				_passthrough_summary: {
					description: data_model.schema.metric.type.object.options.summary.description
					tags: {
						"*": {
							description: "Any tags present on the metric."
							examples: [_values.local_host]
							required: false
						}
					}
					type: "gauge"
				}
			}
		}

		telemetry: metrics: {
			// Default metrics for each component
			vector_events_processed_total: _vector_events_processed_total
		}
	}}
}
