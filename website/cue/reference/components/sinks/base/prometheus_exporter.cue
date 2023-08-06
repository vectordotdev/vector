package metadata

base: components: sinks: prometheus_exporter: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/about/under-the-hood/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type: object: options: enabled: {
			description: """
				Whether or not end-to-end acknowledgements are enabled.

				When enabled for a sink, any source connected to that sink, where the source supports
				end-to-end acknowledgements as well, waits for events to be acknowledged by the sink
				before acknowledging them at the source.

				Enabling or disabling acknowledgements at the sink level takes precedence over any global
				[`acknowledgements`][global_acks] configuration.

				[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
				"""
			required: false
			type: bool: {}
		}
	}
	address: {
		description: """
			The address to expose for scraping.

			The metrics are exposed at the typical Prometheus exporter path, `/metrics`.
			"""
		required: false
		type: string: {
			default: "0.0.0.0:9598"
			examples: ["192.160.0.10:9598"]
		}
	}
	auth: {
		description: """
			Configuration of the authentication strategy for HTTP requests.

			HTTP authentication should be used with HTTPS only, as the authentication credentials are passed as an
			HTTP header without any additional encryption beyond what is provided by the transport itself.
			"""
		required: false
		type: object: options: {
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
				}
			}
			token: {
				description:   "The bearer authentication token."
				relevant_when: "strategy = \"bearer\""
				required:      true
				type: string: {}
			}
			user: {
				description:   "The basic authentication username."
				relevant_when: "strategy = \"basic\""
				required:      true
				type: string: examples: ["${USERNAME}", "username"]
			}
		}
	}
	buckets: {
		description: """
			Default buckets to use for aggregating [distribution][dist_metric_docs] metrics into histograms.

			[dist_metric_docs]: https://vector.dev/docs/about/under-the-hood/architecture/data-model/metric/#distribution
			"""
		required: false
		type: array: {
			default: [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
			items: type: float: {}
		}
	}
	default_namespace: {
		description: """
			The default namespace for any metrics sent.

			This namespace is only used if a metric has no existing namespace. When a namespace is
			present, it is used as a prefix to the metric name, and separated with an underscore (`_`).

			It should follow the Prometheus [naming conventions][prom_naming_docs].

			[prom_naming_docs]: https://prometheus.io/docs/practices/naming/#metric-names
			"""
		required: false
		type: string: {}
	}
	distributions_as_summaries: {
		description: """
			Whether or not to render [distributions][dist_metric_docs] as an [aggregated histogram][prom_agg_hist_docs] or  [aggregated summary][prom_agg_summ_docs].

			While distributions as a lossless way to represent a set of samples for a
			metric is supported, Prometheus clients (the application being scraped, which is this sink) must
			aggregate locally into either an aggregated histogram or aggregated summary.

			[dist_metric_docs]: https://vector.dev/docs/about/under-the-hood/architecture/data-model/metric/#distribution
			[prom_agg_hist_docs]: https://prometheus.io/docs/concepts/metric_types/#histogram
			[prom_agg_summ_docs]: https://prometheus.io/docs/concepts/metric_types/#summary
			"""
		required: false
		type: bool: default: false
	}
	flush_period_secs: {
		description: """
			The interval, in seconds, on which metrics are flushed.

			On the flush interval, if a metric has not been seen since the last flush interval, it is
			considered expired and is removed.

			Be sure to configure this value higher than your client’s scrape interval.
			"""
		required: false
		type: uint: {
			default: 60
			unit:    "seconds"
		}
	}
	quantiles: {
		description: """
			Quantiles to use for aggregating [distribution][dist_metric_docs] metrics into a summary.

			[dist_metric_docs]: https://vector.dev/docs/about/under-the-hood/architecture/data-model/metric/#distribution
			"""
		required: false
		type: array: {
			default: [0.5, 0.75, 0.9, 0.95, 0.99]
			items: type: float: {}
		}
	}
	suppress_timestamp: {
		description: """
			Suppresses timestamps on the Prometheus output.

			This can sometimes be useful when the source of metrics leads to their timestamps being too
			far in the past for Prometheus to allow them, such as when aggregating metrics over long
			time periods, or when replaying old metrics from a disk buffer.
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
			enabled: {
				description: """
					Whether or not to require TLS for incoming or outgoing connections.

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

					If enabled, certificates must not be expired and must be issued by a trusted
					issuer. This verification operates in a hierarchical manner, checking that the leaf certificate (the
					certificate presented by the client/server) is not only valid, but that the issuer of that certificate is also valid, and
					so on until the verification process reaches a root certificate.

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
