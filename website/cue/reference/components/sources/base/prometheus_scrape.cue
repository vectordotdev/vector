package metadata

base: components: sources: prometheus_scrape: configuration: {
	auth: {
		description: """
			Configuration of the authentication strategy for HTTP requests.

			HTTP authentication should be used with HTTPS only, as the authentication credentials are passed as an
			HTTP header without any additional encryption beyond what is provided by the transport itself.
			"""
		required: false
		type: object: options: {
			client_id: {
				description:   "The client id."
				relevant_when: "strategy = \"o_auth2\""
				required:      true
				type: string: examples: ["client_id"]
			}
			client_secret: {
				description:   "The sensitive client secret."
				relevant_when: "strategy = \"o_auth2\""
				required:      false
				type: string: examples: ["client_secret"]
			}
			grace_period: {
				description: """
					The grace period configuration for a bearer token.
					To avoid random authorization failures caused by expired token exception,
					we will acquire new token, some time (grace period) before current token will be expired,
					because of that, we will always execute request with fresh enough token.
					"""
				relevant_when: "strategy = \"o_auth2\""
				required:      false
				type: uint: {
					default: 300
					examples: [300]
					unit: "seconds"
				}
			}
			password: {
				description:   "The basic authentication password."
				relevant_when: "strategy = \"basic\""
				required:      true
				type: string: examples: ["${PASSWORD}", "password"]
			}
			strategy: {
				description: "The authentication strategy to use."
				required:    true
				type: string: enum: {
					basic: """
						Basic authentication.

						The username and password are concatenated and encoded via [base64][base64].

						[base64]: https://en.wikipedia.org/wiki/Base64
						"""
					bearer: """
						Bearer authentication.

						The bearer token value (OAuth2, JWT, etc.) is passed as-is.
						"""
					o_auth2: """
						Authentication based on OAuth 2.0 protocol.

						This strategy allows to dynamically acquire and use token based on provided parameters.
						Both standard client_credentials and mTLS extension is supported, for standard client_credentials just provide both
						client_id and client_secret parameters:

						# Example

						```yaml
						strategy:
						 strategy: "o_auth2"
						 client_id: "client.id"
						 client_secret: "secret-value"
						 token_endpoint: "https://yourendpoint.com/oauth/token"
						```
						In case you want to use mTLS extension [rfc8705](https://datatracker.ietf.org/doc/html/rfc8705), provide desired key and certificate,
						together with client_id (with no client_secret parameter).

						# Example

						```yaml
						strategy:
						 strategy: "o_auth2"
						 client_id: "client.id"
						 token_endpoint: "https://yourendpoint.com/oauth/token"
						tls:
						 crt_path: cert.pem
						 key_file: key.pem
						```
						"""
				}
			}
			token: {
				description:   "The bearer authentication token."
				relevant_when: "strategy = \"bearer\""
				required:      true
				type: string: {}
			}
			token_endpoint: {
				description:   "Token endpoint location, required for token acquisition."
				relevant_when: "strategy = \"o_auth2\""
				required:      true
				type: string: examples: ["https://auth.provider/oauth/token"]
			}
			user: {
				description:   "The basic authentication username."
				relevant_when: "strategy = \"basic\""
				required:      true
				type: string: examples: ["${USERNAME}", "username"]
			}
		}
	}
	endpoint_tag: {
		description: """
			The tag name added to each event representing the scraped instance's endpoint.

			The tag value is the endpoint of the scraped instance.
			"""
		required: false
		type: string: {}
	}
	endpoints: {
		description: "Endpoints to scrape metrics from."
		required:    true
		type: array: items: type: string: examples: ["http://localhost:9090/metrics"]
	}
	honor_labels: {
		description: """
			Controls how tag conflicts are handled if the scraped source has tags to be added.

			If `true`, the new tag is not added if the scraped metric has the tag already. If `false`, the conflicting tag
			is renamed by prepending `exported_` to the original name.

			This matches Prometheus’ `honor_labels` configuration.
			"""
		required: false
		type: bool: default: false
	}
	instance_tag: {
		description: """
			The tag name added to each event representing the scraped instance's `host:port`.

			The tag value is the host and port of the scraped instance.
			"""
		required: false
		type: string: {}
	}
	query: {
		description: """
			Custom parameters for the scrape request query string.

			One or more values for the same parameter key can be provided. The parameters provided in this option are
			appended to any parameters manually provided in the `endpoints` option. This option is especially useful when
			scraping the `/federate` endpoint.
			"""
		required: false
		type: object: {
			examples: [{
				"match[]": ["{job=\"somejob\"}", "{__name__=~\"job:.*\"}"]
			}]
			options: "*": {
				description: "A query string parameter."
				required:    true
				type: array: items: type: string: {}
			}
		}
	}
	scrape_interval_secs: {
		description: """
			The interval between scrapes. Requests are run concurrently so if a scrape takes longer
			than the interval a new scrape will be started. This can take extra resources, set the timeout
			to a value lower than the scrape interval to prevent this from happening.
			"""
		required: false
		type: uint: {
			default: 15
			unit:    "seconds"
		}
	}
	scrape_timeout_secs: {
		description: "The timeout for each scrape request."
		required:    false
		type: float: {
			default: 5.0
			unit:    "seconds"
		}
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type: object: options: {
			alpn_protocols: {
				description: """
					Sets the list of supported ALPN protocols.

					Declare the supported ALPN protocols, which are used during negotiation with peer. They are prioritized in the order
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
					so on until the verification process reaches a root certificate.

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
