package metadata

components: {
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
				description: "Checks whether an IP field is contained within a given [IP CIDR][urls.cidr] (works with IPv4 and IPv6). This may be a single string or a list of strings, in which case this evaluates to true if the IP field is contained within any of the CIDRs in the list."
				required:    false
				warnings: []
				type: string: {
					default: null
					examples: ["10.0.0.0/8", "2000::/10", "192.168.0.0/16"]
				}
			}
			"*.regex": {
				common:      true
				description: "Checks whether a string field matches a [regular expression][urls.regex]. Vector uses the [documented Rust Regex syntax][urls.rust_regex_syntax]. Note that this condition is considerably more expensive than a regular string match (such as `starts_with` or `contains`) so the use of those conditions are preferred where possible."
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

	{[Kind=string]: [Name=string]: {
		kind: string

		features: {
			if kind == "source" || kind == "sink" {
				tls: {
					enabled: bool

					if enabled {
						can_enable:             bool
						can_verify_certificate: bool
						if kind == "sink" {
							can_verify_hostname: bool
						}
						enabled_default: bool
					}
				}
			}
		}

		configuration: {
			if (kind == "source" || kind == "sink") {
				if features.tls.enabled {
					tls: {
						common:      false
						description: "Configures the TLS options for connections from this source."
						required:    false
						type: object: options: {
							if features.tls.can_enable {
								enabled: {
									common:      false
									description: "Require TLS for incoming connections. If this is set, an identity certificate is also required."
									required:    false
									type: bool: default: features.tls.enabled_default
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

							if features.tls.enabled_default {
								verify_certificate: {
									common:      false
									description: "If `true`, Vector will require a TLS certificate from the connecting host and terminate the connection if the certificate is not valid. If `false` (the default), Vector will not request a certificate from the client."
									required:    false
									type: bool: default: false
								}
							}

							if kind == "sink" {
								if features.tls.can_verify_hostname {
									verify_hostname: {
										common:      false
										description: "If `true` (the default), Vector will validate the configured remote host name against the remote host's TLS certificate. Do NOT set this to `false` unless you understand the risks of not verifying the remote host name."
										required:    false
										type: bool: default: true
									}
								}
							}
						}
					}
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

		how_it_works: {
			environment_variables: {
				title: "Environment Variables"
				body: #"""
					Environment variables are supported through all of Vector's
					configuration. Simply add ${MY_ENV_VAR} in your Vector
					configuration file and the variable will be replaced before being
					evaluated.

					Learn more in the [configuration manual](/docs/manual/setup/configuration).
					"""#
			}

			if (kind == "source" || kind == "sink") {
				if features.tls.enabled {
					tls: {
						title: "Transport Layer Security (TLS)"
						body: #"""
                  Vector uses [Openssl][urls.openssl] for TLS protocols. You can
                  enable and adjust TLS behavior via the `tls.*` options.
                  """#
					}
				}
			}
		}
	}}
}
