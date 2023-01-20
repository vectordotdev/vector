package metadata

base: components: sources: splunk_hec: configuration: {
	acknowledgements: {
		description: "Acknowledgement configuration for the `splunk_hec` source."
		required:    false
		type: object: options: {
			ack_idle_cleanup: {
				description: """
					Whether or not to remove channels after idling for `max_idle_time` seconds.

					A channel is idling if it is not used for sending data or querying ack statuses.
					"""
				required: false
				type: bool: default: false
			}
			enabled: {
				description: "Enables end-to-end acknowledgements."
				required:    false
				type: bool: {}
			}
			max_idle_time: {
				description: """
					The amount of time, in seconds, a channel is allowed to idle before removal.

					Channels can potentially idle for longer than this setting but clients should not rely on such behavior.

					Minimum of `1`.
					"""
				required: false
				type: uint: default: 300
			}
			max_number_of_ack_channels: {
				description: """
					The maximum number of Splunk HEC channels clients can use with this source.

					Minimum of `1`.
					"""
				required: false
				type: uint: default: 1000000
			}
			max_pending_acks: {
				description: """
					The maximum number of ack statuses pending query across all channels.

					Equivalent to the `max_number_of_acked_requests_pending_query` Splunk HEC setting.

					Minimum of `1`.
					"""
				required: false
				type: uint: default: 10000000
			}
			max_pending_acks_per_channel: {
				description: """
					The maximum number of ack statuses pending query for a single channel.

					Equivalent to the `max_number_of_acked_requests_pending_query_per_ack_channel` Splunk HEC setting.

					Minimum of `1`.
					"""
				required: false
				type: uint: default: 1000000
			}
		}
	}
	address: {
		description: """
			The socket address to listen for connections on.

			The address _must_ include a port.
			"""
		required: false
		type: string: default: "0.0.0.0:8088"
	}
	store_hec_token: {
		description: """
			Whether or not to forward the Splunk HEC authentication token with events.

			If set to `true`, when incoming requests contain a Splunk HEC token, the token used will kept in the
			event metadata and be preferentially used if the event is sent to a Splunk HEC sink.
			"""
		required: false
		type: bool: default: false
	}
	tls: {
		description: "Configures the TLS options for incoming/outgoing connections."
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
			enabled: {
				description: """
					Whether or not to require TLS for incoming/outgoing connections.

					When enabled and used for incoming connections, an identity certificate is also required. See `tls.crt_file` for
					more information.
					"""
				required: false
				type: bool: {}
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
	token: {
		description: """
			Optional authorization token.

			If supplied, incoming requests must supply this token in the `Authorization` header, just as a client would if
			it was communicating with the Splunk HEC endpoint directly.

			If _not_ supplied, the `Authorization` header will be ignored and requests will not be authenticated.
			"""
		required: false
		type: string: {}
	}
	valid_tokens: {
		description: """
			Optional list of valid authorization tokens.

			If supplied, incoming requests must supply one of these tokens in the `Authorization` header, just as a client
			would if it was communicating with the Splunk HEC endpoint directly.

			If _not_ supplied, the `Authorization` header will be ignored and requests will not be authenticated.
			"""
		required: false
		type: array: items: type: string: {}
	}
}
