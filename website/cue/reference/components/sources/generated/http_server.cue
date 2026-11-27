package metadata

generated: components: sources: http_server: configuration: {
	acknowledgements: {
		deprecated: true
		description: """
			Controls how acknowledgements are handled by this source.

			This setting is **deprecated** in favor of enabling `acknowledgements` at the [global][global_acks] or sink level.

			Enabling or disabling acknowledgements at the source level has **no effect** on acknowledgement behavior.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["bool_or_struct::vector_core::config::SourceAcknowledgementsConfig"]
	}
	address: {
		description: """
			The socket address to listen for connections on.

			It _must_ include a port.
			"""
		required: true
		type: string: examples: ["0.0.0.0:80", "localhost:80"]
	}
	auth: {
		description: """
			HTTP authentication configuration.

			Use HTTP authentication with HTTPS only. The authentication credentials are passed as an
			HTTP header without any additional encryption beyond what is provided by the transport itself.

			When using the `custom` strategy, the VRL program may write `%field = value` to enrich
			authenticated events. These metadata fields are injected into the event body (legacy
			namespace) or under `http_server.<field>` in event metadata (Vector namespace).
			"""
		required: false
		type:     _schemaDefinitions["core::option::Option<vector::common::http::server_auth::HttpServerAuthConfig>"]
	}
	decoding: {
		description: """
			Configures how events are decoded from raw bytes. Note some decoders can also determine the event output
			type (log, metric, trace).
			"""
		required: false
		type:     _schemaDefinitions["codecs::decoding::DeserializerConfig"]
	}
	framing: {
		description: """
			Framing configuration.

			Framing handles how events are separated when encoded in a raw byte form, where each event is
			a frame that must be prefixed, or delimited, in a way that marks where an event begins and
			ends within the byte stream.
			"""
		required: false
		type:     _schemaDefinitions["codecs::decoding::FramingConfig"]
	}
	headers: {
		description: """
			A list of HTTP headers to include in the log event.

			Accepts the wildcard (`*`) character for headers matching a specified pattern.

			Specifying "*" results in all headers included in the log event.

			These headers are not included in the JSON payload if a field with a conflicting name exists.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: examples: ["User-Agent", "X-My-Custom-Header", "X-*", "*"]
		}
	}
	host_key: {
		description: "If set, the name of the log field used to add the remote IP to each event"
		required:    false
		type: string: {
			default: ""
			examples: ["hostname"]
		}
	}
	keepalive: {
		description: "Configuration of HTTP server keepalive parameters."
		required:    false
		type:        _schemaDefinitions["vector::http::KeepaliveConfig"]
	}
	method: {
		description: "Specifies the action of the HTTP request."
		required:    false
		type: string: {
			default: "POST"
			enum: {
				DELETE:  "HTTP DELETE method."
				GET:     "HTTP GET method."
				HEAD:    "HTTP HEAD method."
				OPTIONS: "HTTP OPTIONS method."
				PATCH:   "HTTP PATCH method."
				POST:    "HTTP POST method."
				PUT:     "HTTP Put method."
			}
		}
	}
	path: {
		description: "The URL path on which log event POST requests are sent."
		required:    false
		type: string: {
			default: "/"
			examples: ["/event/path", "/logs"]
		}
	}
	path_key: {
		description: "The event key in which the requested URL path used to send the request is stored."
		required:    false
		type: string: {
			default: "path"
			examples: ["vector_http_path"]
		}
	}
	query_parameters: {
		description: """
			A list of URL query parameters to include in the log event.

			Accepts the wildcard (`*`) character for query parameters matching a specified pattern.

			Specifying "*" results in all query parameters included in the log event.

			These override any values included in the body with conflicting names.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: examples: ["application", "source", "param*", "*"]
		}
	}
	reject_code: {
		description: """
			The HTTP status code returned to the client when the `response_source` VRL program calls
			`abort`. Use `abort "message"` in your VRL program to suppress event forwarding and return
			this status code immediately, bypassing sink acknowledgements entirely.

			The appropriate code depends on the reason for rejection: for example, `400` for invalid
			input, `403` for an unauthorized source, or `429` for a rate limit. Defaults to `400`.

			To vary the status code per `abort` call rather than using a single fixed value, pass a
			JSON-encoded object as the abort message instead of a plain string:
			`abort encode_json({ "status": 403, "body": "forbidden" })`. The object accepts the same
			optional fields as the `response_source` return value: `status`, `body`, and `headers`.
			When `status` is omitted from the object, this `reject_code` is used as the fallback.
			"""
		required: false
		type: uint: {
			default: 400
			examples: [400, 403]
		}
	}
	response_code: {
		description: "Specifies the HTTP response status code that will be returned on successful requests."
		required:    false
		type: uint: {
			default: 200
			examples: [
				202
			]
		}
	}
	response_source: {
		description: """
			A [VRL] program to generate the HTTP response sent back to the client.

			The program receives the decoded, enriched events where `.` is an array of event objects.
			It must return either:
			- A string, used directly as the response body with the configured `response_code`.
			- An object with optional fields: `status` (integer), `body` (string), `headers` (object).

			## Event types

			The program only sees log events. When a decoder that emits metric or trace events is
			configured (for example `native`, `native_json`, or `otlp`), those events are still
			forwarded to the sink but are not included in `.`. Programs that count or inspect events
			(for example `length(.)`) therefore account only for log events.

			## Aborting and early rejection

			Call `abort` in the program to suppress event forwarding entirely and respond immediately,
			bypassing sink acknowledgements. The abort message becomes the response body and
			[`reject_code`][Self::reject_code] is used as the status code.

			To control both the status code and body per `abort` call, pass a JSON-encoded object as
			the message: `abort encode_json({ "status": 403, "body": "forbidden" })`. The object
			accepts the same optional fields as the normal return value: `status`, `body`, and
			`headers`. Any field that is omitted falls back to the same default as the normal path
			(`status` defaults to `reject_code`, `body` defaults to empty, `headers` to none).

			## Acknowledgements and sink failures

			When [acknowledgements] are enabled, the response defined by this program is only sent to
			the client if the downstream sink successfully delivers the events (`Delivered`). If the
			sink reports a failure (`Errored` or `Rejected`), the configured response is discarded and
			the client receives a `500 Internal Server Error` or `400 Bad Request` instead.

			Responses returned via `abort` are not subject to this. They are sent immediately before
			any events reach the sink, so acknowledgement results can never override them.

			## Log namespace

			When [`log_namespace`][Self::log_namespace] is set to `vector`, request metadata such as
			the request path, headers, query parameters, and source IP are stored in event metadata
			rather than in the event fields that the program can access via `.`. Programs that rely
			on inspecting this context should use the default `legacy` log namespace, where all
			metadata is written directly into the event fields before the program runs.

			The remote IP is only present when [`host_key`][Self::host_key] is set, under that field
			name (for example `.hostname` when `host_key = "hostname"`).

			[acknowledgements]: https://vector.dev/docs/about/under-the-hood/architecture/end-to-end-acknowledgements/
			[VRL]: https://vector.dev/docs/reference/vrl
			"""
		required: false
		type: string: {
			examples: ["encode_json({ \"ids\": map_values(.) -> |e| { e.id } })", """
				parsed, err = parse_json(.[0].body)
				if err != null {
				  { "status": 400, "body": "invalid JSON" }
				} else {
				  { "status": 202, "body": encode_json(parsed), "headers": { "x-request-id": .[0].request_id } }
				}
				""", """
				row, err = get_enrichment_table_record("ip_allowlist", { "ip": .[0].hostname })
				if err != null {
				  abort encode_json({ "status": 403, "body": "source address not permitted" })
				} else {
				  { "status": 202, "body": encode_json({ "accepted": true }) }
				}
				"""]
			syntax: "remap_program"
		}
	}
	strict_path: {
		description: """
			Whether or not to treat the configured `path` as an absolute path.

			If set to `true`, only requests using the exact URL path specified in `path` are accepted. Otherwise,
			requests sent to a URL path that starts with the value of `path` are accepted.

			With `strict_path` set to `false` and `path` set to `""`, the configured HTTP source accepts requests from
			any URL path.
			"""
		required: false
		type: bool: default: true
	}
	tls: {
		description: "Configures the TLS options for incoming/outgoing connections."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsEnableableConfig>"]
	}
}
