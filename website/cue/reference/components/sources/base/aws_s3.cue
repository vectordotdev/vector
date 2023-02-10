package metadata

base: components: sources: aws_s3: configuration: {
	acknowledgements: {
		deprecated: true
		description: """
			Controls how acknowledgements are handled by this source.

			This setting is **deprecated** in favor of enabling `acknowledgements` at the [global][global_acks] or sink level.

			Enabling or disabling acknowledgements at the source level has **no effect** on acknowledgement behavior.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
			[e2e_acks]: https://vector.dev/docs/about/under-the-hood/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type: object: options: enabled: {
			description: "Whether or not end-to-end acknowledgements are enabled for this source."
			required:    false
			type: bool: {}
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

					Relevant when the default credentials chain is used or `assume_role`.
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
				type: string: examples: ["develop"]
			}
			region: {
				description: """
					The [AWS region][aws_region] to send STS requests to.

					If not set, this will default to the configured region
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
		}
	}
	compression: {
		description: "The compression scheme used for decompressing objects retrieved from S3."
		required:    false
		type: string: {
			default: "auto"
			enum: {
				auto: """
					Automatically attempt to determine the compression scheme.

					The compression scheme of the object is determined from its `Content-Encoding` and
					`Content-Type` metadata, as well as the key suffix (for example, `.gz`).

					It is set to `none` if the compression scheme cannot be determined.
					"""
				gzip: "GZIP."
				none: "Uncompressed."
				zstd: "ZSTD."
			}
		}
	}
	endpoint: {
		description: "Custom endpoint for use with AWS-compatible services."
		required:    false
		type: string: examples: ["http://127.0.0.0:5000/path/to/service"]
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

						This is useful in cases such as a Java stack trace, where some indicator in the line (such as leading
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
	region: {
		description: """
			The [AWS region][aws_region] of the target service.

			[aws_region]: https://docs.aws.amazon.com/general/latest/gr/rande.html#regional-endpoints
			"""
		required: false
		type: string: examples: ["us-east-1"]
	}
	sqs: {
		description: """
			Configuration options for SQS.

			Only relevant when `strategy = "sqs"`.
			"""
		required: false
		type: object: options: {
			client_concurrency: {
				description: """
					Number of concurrent tasks to create for polling the queue for messages.

					Defaults to the number of available CPUs on the system.

					Should not typically need to be changed, but it can sometimes be beneficial to raise this
					value when there is a high rate of messages being pushed into the queue and the objects
					being fetched are small. In these cases, system resources may not be fully utilized without
					fetching more messages per second, as the SQS message consumption rate affects the S3 object
					retrieval rate.
					"""
				required: false
				type: uint: {
					examples: [5]
					unit: "tasks"
				}
			}
			delete_message: {
				description: """
					Whether to delete the message once it is processed.

					It can be useful to set this to `false` for debugging or during the initial setup.
					"""
				required: false
				type: bool: default: true
			}
			poll_secs: {
				description: """
					How long to wait while polling the queue for new messages, in seconds.

					Generally should not be changed unless instructed to do so, as if messages are available,
					they will always be consumed, regardless of the value of `poll_secs`.
					"""
				required: false
				type: uint: {
					default: 15
					unit:    "seconds"
				}
			}
			queue_url: {
				description: "The URL of the SQS queue to poll for bucket notifications."
				required:    true
				type: string: examples: ["https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue"]
			}
			tls_options: {
				description: "TLS configuration."
				required:    false
				type: object: options: {
					alpn_protocols: {
						description: """
																Sets the list of supported ALPN protocols.

																Declare the supported ALPN protocols, which are used during negotiation with peer. Prioritized in the order
																they are defined.
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

																If this is set, and is not a PKCS#12 archive, `key_file` must also be set.
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
					verify_certificate: {
						description: """
																Enables certificate verification.

																If enabled, certificates must be valid in terms of not being expired, as well as being issued by a trusted
																issuer. This verification operates in a hierarchical manner, checking that not only the leaf certificate (the
																certificate presented by the client/server) is valid, but also that the issuer of that certificate is valid, and
																so on until reaching a root certificate.

																Relevant for both incoming and outgoing connections.

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
			visibility_timeout_secs: {
				description: """
					The visibility timeout to use for messages, in seconds.

					This controls how long a message is left unavailable after it is received. If a message is received, and
					takes longer than `visibility_timeout_secs` to process and delete the message from the queue, it is made available again for another consumer.

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
	strategy: {
		description: "The strategy to use to consume objects from S3."
		required:    false
		type: string: {
			default: "sqs"
			enum: sqs: """
				Consumes objects by processing bucket notification events sent to an [AWS SQS queue][aws_sqs].

				[aws_sqs]: https://aws.amazon.com/sqs/
				"""
		}
	}
	tls_options: {
		description: "TLS configuration."
		required:    false
		type: object: options: {
			alpn_protocols: {
				description: """
					Sets the list of supported ALPN protocols.

					Declare the supported ALPN protocols, which are used during negotiation with peer. Prioritized in the order
					they are defined.
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

					If this is set, and is not a PKCS#12 archive, `key_file` must also be set.
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
			verify_certificate: {
				description: """
					Enables certificate verification.

					If enabled, certificates must be valid in terms of not being expired, as well as being issued by a trusted
					issuer. This verification operates in a hierarchical manner, checking that not only the leaf certificate (the
					certificate presented by the client/server) is valid, but also that the issuer of that certificate is valid, and
					so on until reaching a root certificate.

					Relevant for both incoming and outgoing connections.

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
