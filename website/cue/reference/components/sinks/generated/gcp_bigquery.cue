package metadata

generated: components: sinks: gcp_bigquery: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["bool_or_struct::vector_core::config::AcknowledgementsConfig"]
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type: object: options: {
			max_bytes: {
				description: """
					The maximum size of a batch that is processed by a sink.

					This is based on the uncompressed size of the batched events, before they are
					serialized or compressed.
					"""
				required: false
				type: uint: {
					default: 10000000
					unit:    "bytes"
				}
			}
			max_events: {
				description: "The maximum size of a batch before it is flushed."
				required:    false
				type: uint: {
					default: 50000
					unit:    "events"
				}
			}
			timeout_secs: {
				description: "The maximum age of a batch before it is flushed."
				required:    false
				type: float: {
					default: 1.0
					unit:    "seconds"
				}
			}
		}
	}
	credentials_path: {
		description: """
			Path to a [service account][gcp_service_account_credentials] credentials JSON file.

			If unset, the `GOOGLE_APPLICATION_CREDENTIALS` environment variable is checked for a filename. If no
			filename is named, an attempt is made to fetch an instance service account for the compute instance the program is
			running on.

			API key authentication is not supported by the BigQuery Storage Write API.

			[gcp_service_account_credentials]: https://cloud.google.com/docs/authentication/production#manually
			"""
		required: false
		type: string: {}
	}
	dataset: {
		description: "The dataset within the project to which to publish events."
		required:    true
		type: string: examples: ["this-is-a-dataset"]
	}
	encoding: {
		description: """
			Encoding configuration.

			Only Protobuf is supported, because the Storage Write API ingests rows as serialized Protobuf messages.
			"""
		required: true
		type: object: options: protobuf: {
			description: "Options for the Protobuf serializer."
			required:    true
			type:        _schemaDefinitions["codecs::encoding::format::protobuf::ProtobufSerializerOptions"]
		}
	}
	endpoint: {
		description: """
			The endpoint to which to publish events.

			The scheme (`http` or `https`) must be specified. No path should be included since the paths defined
			by the [BigQuery Storage Write API][bigquery_write_api] are used.

			The trailing slash `/` must not be included.

			[bigquery_write_api]: https://cloud.google.com/bigquery/docs/write-api
			"""
		required: false
		type: string: {
			default: "https://bigquerystorage.googleapis.com:443"
			examples: ["https://bigquerystorage.googleapis.com:443"]
		}
	}
	project: {
		description: "The project name to which to publish events."
		required:    true
		type: string: examples: ["vector-123456"]
	}
	request: {
		description: """
			Middleware settings for outbound requests.

			Various settings can be configured, such as concurrency and rate limits, timeouts, and retry behavior.

			Note that the retry backoff policy follows the Fibonacci sequence.
			"""
		required: false
		type:     _schemaDefinitions["vector::sinks::util::service::TowerRequestConfig"]
	}
	table: {
		description: "The table within the dataset to which to publish events."
		required:    true
		type: string: examples: ["this-is-a-table"]
	}
	tls: {
		description: """
			TLS configuration.

			TLS is used whenever `endpoint` uses the `https` scheme, verifying the server against the system's
			trusted root certificates. These options customize it.
			"""
		required: false
		type: object: options: {
			ca_file: {
				description: """
					Absolute path to an additional CA certificate file, trusted in addition to the system's root certificates.

					The certificate must be in the DER or PEM (X.509) format. Additionally, the certificate can be provided as an inline string in PEM format.
					"""
				required: false
				type: string: examples: ["/path/to/certificate_authority.crt"]
			}
			crt_file: {
				description: """
					Absolute path to a certificate file used to identify this client.

					The certificate must be in DER, PEM (X.509), or PKCS#12 format. Additionally, the certificate can be provided as
					an inline string in PEM format.

					If this is set _and_ is not a PKCS#12 archive, `key_file` must also be set.
					"""
				required: false
				type: string: examples: ["/path/to/host_certificate.crt"]
			}
			key_file: {
				description: """
					Absolute path to a private key file used to identify this client.

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
					Server name to use for Server Name Indication (SNI) and to verify the server's certificate.

					Defaults to the host of `endpoint`.
					"""
				required: false
				type: string: examples: ["bigquerystorage.googleapis.com"]
			}
		}
	}
}
