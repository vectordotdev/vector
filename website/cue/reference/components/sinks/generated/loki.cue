package metadata

generated: components: sinks: loki: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	auth: {
		description: """
			Configuration of the authentication strategy for HTTP requests.

			HTTP authentication should be used with HTTPS only, as the authentication credentials are passed as an
			HTTP header without any additional encryption beyond what is provided by the transport itself.
			"""
		required: false
		type:     _schemaDefinitions["core::option::Option<vector::http::Auth>"]
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
					default: 1000000
					unit:    "bytes"
				}
			}
			max_events: {
				description: "The maximum size of a batch before it is flushed."
				required:    false
				type: uint: {
					default: 100000
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
	compression: {
		description: """
			Compression configuration.
			Snappy compression implies sending push requests as Protocol Buffers.
			"""
		required: false
		type: string: {
			default: "snappy"
			enum: {
				gzip: """
					[Gzip][gzip] compression.

					[gzip]: https://www.gzip.org/
					"""
				none: "No compression."
				snappy: """
					[Snappy][snappy] compression.

					[snappy]: https://github.com/google/snappy/blob/main/docs/README.md
					"""
				zlib: """
					[Zlib][zlib] compression.

					[zlib]: https://zlib.net/
					"""
				zstd: """
					[Zstandard][zstd] compression.

					[zstd]: https://facebook.github.io/zstd/
					"""
			}
		}
	}
	dangerously_allow_unconfined_template_resolution: {
		description: """
			Disable all template confinement checks for this sink.

			**DANGEROUS — disables a security control.**

			Bypasses both startup validation and runtime confinement for every
			templated field on this sink. When enabled, a log producer that
			controls any field used in a template can write to arbitrary keys,
			paths, or routing destinations. This flag is a full opt-out: it
			disables confinement even for templates that have a usable static
			prefix.
			"""
		required: false
		type: bool: default: false
	}
	encoding: {
		description: """
			Encoding configuration.
			Configures how events are encoded into raw bytes.
			The selected encoding also determines which input types (logs, metrics, traces) are supported.
			"""
		required: true
		type:     _schemaDefinitions["codecs::encoding::config::EncodingConfig"]
	}
	endpoint: {
		description: """
			The base URL of the Loki instance.

			The `path` value is appended to this.
			"""
		required: true
		type: string: examples: ["http://localhost:3100"]
	}
	labels: {
		description: """
			A set of labels that are attached to each batch of events.

			Both keys and values are templateable, which enables you to attach dynamic labels to events.

			Valid label keys include `*`, and prefixes ending with `*`, to allow for the expansion of
			objects into multiple labels. See [Label expansion][label_expansion] for more information.

			Note: If the set of labels has high cardinality, this can cause drastic performance issues
			with Loki. To prevent this from happening, reduce the number of unique label keys and
			values.

			[label_expansion]: https://vector.dev/docs/reference/configuration/sinks/loki/#label-expansion
			"""
		required: true
		type: object: {
			examples: [{
				"event_{{ event_field }}": "value_{{ some_other_event_field }}"
				"pod_labels_*":            "{{ kubernetes.pod_labels }}"
				source:                    "vector"
			}]
			options: "*": {
				description: "A Loki label."
				required:    true
				type: string: syntax: "template"
			}
		}
	}
	out_of_order_action: {
		description: """
			Out-of-order event behavior.

			Some sources may generate events with timestamps that aren't in chronological order. Even though the
			sink sorts the events before sending them to Loki, there is a chance that another event could come in
			that is out of order with the latest events sent to Loki. Prior to Loki 2.4.0, this
			was not supported and would result in an error during the push request.

			If you're using Loki 2.4.0 or newer, `Accept` is the preferred action, which lets Loki handle
			any necessary sorting/reordering. If you're using an earlier version, then you must use `Drop`
			or `RewriteTimestamp` depending on which option makes the most sense for your use case.
			"""
		required: false
		type: string: {
			default: "accept"
			enum: {
				accept: """
					Accept the event.

					The event is not dropped and is sent without modification.

					Requires Loki 2.4.0 or newer.
					"""
				drop:              "Drop the event."
				rewrite_timestamp: "Rewrite the timestamp of the event to the timestamp of the latest event seen by the sink."
			}
		}
	}
	path: {
		description: "The path to use in the URL of the Loki instance."
		required:    false
		type: string: default: "/loki/api/v1/push"
	}
	remove_label_fields: {
		description: "Whether or not to delete fields from the event when they are used as labels."
		required:    false
		type: bool: default: false
	}
	remove_structured_metadata_fields: {
		description: "Whether or not to delete fields from the event when they are used in structured metadata."
		required:    false
		type: bool: default: false
	}
	remove_timestamp: {
		description: """
			Whether or not to remove the timestamp from the event payload.

			The timestamp is still sent as event metadata for Loki to use for indexing.
			"""
		required: false
		type: bool: default: true
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
	structured_metadata: {
		description: """
			Structured metadata that is attached to each batch of events.

			Both keys and values are templateable, which enables you to attach dynamic structured metadata to events.

			Valid metadata keys include `*`, and prefixes ending with `*`, to allow for the expansion of
			objects into multiple metadata entries. This follows the same logic as [Label expansion][label_expansion].

			[label_expansion]: https://vector.dev/docs/reference/configuration/sinks/loki/#label-expansion
			"""
		required: false
		type: object: {
			examples: [{
				"event_{{ event_field }}": "value_{{ some_other_event_field }}"
				"pod_labels_*":            "{{ kubernetes.pod_labels }}"
				source:                    "vector"
			}]
			options: "*": {
				description: "Loki structured metadata."
				required:    true
				type: string: syntax: "template"
			}
		}
	}
	tenant_id: {
		description: """
			The [tenant ID][tenant_id] to specify in requests to Loki.

			When running Loki locally, a tenant ID is not required.

			[tenant_id]: https://grafana.com/docs/loki/latest/operations/multi-tenancy/
			"""
		required: false
		type: string: {
			examples: ["some_tenant_id", "{{ event_field }}"]
			syntax: "template"
		}
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
}
