package metadata

_schemaDefinitions: "core::option::Option<vector_core::tls::settings::TlsSourceConfig>": object: options: {
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
	client_metadata_key: {
		description: "Event field for client certificate metadata."
		required:    false
		type: string: {}
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
	enabled: {
		description: """
			Whether to require TLS for incoming or outgoing connections.

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
	max_tls_version: {
		description: """
			Maximum TLS protocol version to negotiate.

			Peers are never offered a version newer than this. This is rarely needed, and is intended
			for working around peers that advertise support for a version they cannot actually
			negotiate.

			When unset, the maximum is whatever the underlying TLS library permits. Note that for
			components that accept connections, TLS v1.3 is disabled unless either this option or
			`min_tls_version` is set.
			"""
		required: false
		type: string: enum: {
			TLSv1: """
				TLS v1.0.

				Deprecated by [RFC 8996][rfc_8996]. Only select this to interoperate with legacy peers
				that cannot be upgraded.

				[rfc_8996]: https://datatracker.ietf.org/doc/html/rfc8996
				"""
			"TLSv1.1": """
				TLS v1.1.

				Deprecated by [RFC 8996][rfc_8996]. Only select this to interoperate with legacy peers
				that cannot be upgraded.

				[rfc_8996]: https://datatracker.ietf.org/doc/html/rfc8996
				"""
			"TLSv1.2": "TLS v1.2."
			"TLSv1.3": "TLS v1.3."
		}
	}
	min_tls_version: {
		description: """
			Minimum TLS protocol version to negotiate.

			Peers that cannot negotiate at least this version are rejected during the handshake.

			When unset, the minimum is whatever the underlying TLS library permits, which currently
			includes the deprecated TLS v1.0 and v1.1. Set this to `TLSv1.2` to refuse them.

			Components that accept connections do not offer TLS v1.3 by default. Setting either this
			option or `max_tls_version` enables every version within the resulting window, so
			`min_tls_version: TLSv1.2` also makes TLS v1.3 available.
			"""
		required: false
		type: string: enum: {
			TLSv1: """
				TLS v1.0.

				Deprecated by [RFC 8996][rfc_8996]. Only select this to interoperate with legacy peers
				that cannot be upgraded.

				[rfc_8996]: https://datatracker.ietf.org/doc/html/rfc8996
				"""
			"TLSv1.1": """
				TLS v1.1.

				Deprecated by [RFC 8996][rfc_8996]. Only select this to interoperate with legacy peers
				that cannot be upgraded.

				[rfc_8996]: https://datatracker.ietf.org/doc/html/rfc8996
				"""
			"TLSv1.2": "TLS v1.2."
			"TLSv1.3": "TLS v1.3."
		}
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
