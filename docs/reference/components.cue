package metadata

components: {
	// `#Classes` represent various `#Components` classifications.
	#Classes: {
		_args: kind: string
		let Args = _args

		// `commonly_used` specifies if the component is commonly used or not.
		// Setting this to `true` will surface the component from other,
		// less commonly used, components.
		commonly_used: bool

		if Args.kind == "source" || Args.kind == "sink" {
			delivery: #DeliveryStatus
		}

		if Args.kind == "source" {
			// `deployment_roles` clarify when the component should be used under
			// different deployment contexts.
			deployment_roles: [...#DeploymentRole]
		}
		development: #DevelopmentStatus

		// `egress_method` documents how the component outputs events.
		egress_method: #EgressMethod

		if Args.kind == "sink" {
			// `service_providers` specify the service providers that support
			// and host this service. This helps users find relevant sinks.
			//
			// For example, "AWS" is a service provider for many services, and
			// a user on AWS can use this to filter for AWS supported
			// components.
			service_providers: [string, ...string] | *[]
		}

		stateful: bool
	}

	#Component: {
		// `kind` specified the component kind. This is set automatically.
		kind: #ComponentKind
		let Kind = kind

		installation?: {
			platform_name: string | null
		}

		configuration: #Schema

		// `description` describes the components with a single paragraph.
		// It is used for SEO purposes and should be full of relevant keywords.
		description?: =~"[.]$"

		env_vars: #EnvVars

		// `alias` is used to register a component's former name when it
		// undergoes a name change.
		alias?: !=""

		// `type` is the component identifier. This is set automatically.
		type: string

		// `classes` represent the various classifications for this component
		classes: #Classes & {_args: kind: Kind}

		// `examples` demonstrates various ways to use the component using an
		// input, output, and example configuration.
		#ExampleConfig: close({
			title:    string
			context?: string
			"configuration": {
				for k, v in configuration {
					"\( k )"?: _ | *null
				}
			}

			if Kind == "source" {
				input: string
			}

			if Kind != "source" {
				input: #Event | [#Event, ...#Event]
			}

			if Kind == "sink" {
				output: string
			}

			if Kind != "sink" {
				output: #Event | [#Event, ...#Event] | null
			}

			notes?: string
		})

		examples?: [#ExampleConfig, ...#ExampleConfig]

		// `features` describes the various supported features of the component.
		// Setting these helps to reduce boilerplate.
		//
		// For example, the `tls` feature will automatically add the appropriate
		// `tls` options and `how_it_works` sections.
		features: #Features & {_args: {egress_method: classes.egress_method, kind: Kind}}

		// `how_it_works` contain sections that further describe the component's
		// behavior. This is like a mini-manual for the component and should help
		// answer any obvious questions the user might have. Options can link
		// to these sections for deeper explanations of behavior.
		how_it_works: #HowItWorks

		if Kind != "source" {
			input: #Input
		}

		if Kind != "sink" {
			// `output` documents output of the component. This is very important
			// as it communicate which events and fields are emitted.
			output: #Output
		}

		// `support` communicates the varying levels of support of the component.
		support: #Support & {_args: kind: Kind}

		// `title` is the human friendly title for the component.
		//
		// For example, the `http` sink has a `HTTP` title.
		title: string

		// Platform-specific policies, e.g. AWS IAM policies, that are
		// required or recommended when using the component.
		permissions?: iam: [#IAM, ...#IAM]

		// Telemetry produced by the component
		telemetry: metrics: #MetricOutput
	}

	// `#ComponentKind` represent the kind of component.
	#ComponentKind: "sink" | "source" | "transform"

	#Components: [Type=string]: #Component & {
		type: Type
	}

	// `#EgressMethod` specified how a component outputs events.
	//
	// * `batch` - one or more events at a time
	// * `dynamic` - can switch between batch and stream based on configuration.
	// * `expose` - exposes data, ex: prometheus_exporter sink
	// * `stream` - one event at a time
	#EgressMethod: "batch" | "dynamic" | "expose" | "stream"

	#EnvVars: #Schema & {[Type=string]: {
		common:   true
		required: false
	}}

	#Features: {
		_args: {
			egress_method: string
			kind:          string
		}
		let Args = _args

		if Args.kind == "source" {
			collect?:  #FeaturesCollect
			generate?: #FeaturesGenerate
			multiline: #FeaturesMultiline
			encoding?: #FeaturesEncoding
			receive?:  #FeaturesReceive
		}

		if Args.kind == "transform" {
			convert?:  #FeaturesConvert
			enrich?:   #FeaturesEnrich
			filter?:   #FeaturesFilter
			parse?:    #FeaturesParse
			program?:  #FeaturesProgram
			reduce?:   #FeaturesReduce
			route?:    #FeaturesRoute
			sanitize?: #FeaturesSanitize
			shape?:    #FeaturesShape
		}

		if Args.kind == "sink" {
			// `buffer` describes how the component buffers data.
			buffer: close({
				enabled: bool | string
			})

			// `healtcheck` notes if a component offers a healthcheck on boot.
			healthcheck: close({
				enabled: bool
			})

			exposes?: #FeaturesExpose
			send?:    #FeaturesSend & {_args: Args}
		}

		descriptions: [Name=string]: string
	}

	#FeaturesCollect: {
		checkpoint: close({
			enabled: bool
		})

		from?: {
			service:    #Service
			interface?: #Interface
		}

		tls?: #FeaturesTLS & {_args: {mode: "connect"}}
	}

	#FeaturesConvert: {
	}

	#FeaturesEnrich: {
		from: service: close({
			name:     string
			url:      string
			versions: string | null
		})
	}

	#FeaturesExpose: {
		tls: #FeaturesTLS & {_args: {mode: "accept"}}

		for: {
			service:    #Service
			interface?: #Interface
		}
	}

	#FeaturesFilter: {
	}

	#FeaturesGenerate: {
	}

	#FeaturesSendBufferBytes: {
		enabled:        bool
		relevant_when?: string
	}

	#FeaturesReceiveBufferBytes: {
		enabled:        bool
		relevant_when?: string
	}

	#FeaturesKeepalive: {
		enabled: bool
	}

	#FeaturesMultiline: {
		enabled: bool
	}

	#FeaturesEncoding: {
		enabled: bool
	}

	#FeaturesParse: {
		format: close({
			name:     string
			url:      string | null
			versions: string | null
		})
	}

	#FeaturesProgram: {
		runtime: #Runtime
	}

	#FeaturesReceive: {
		from?: {
			service:    #Service
			interface?: #Interface
		}

		keepalive?: #FeaturesKeepalive

		receive_buffer_bytes?: #FeaturesReceiveBufferBytes

		tls: #FeaturesTLS & {_args: {mode: "accept"}}
	}

	#FeaturesReduce: {
	}

	#FeaturesRoute: {
	}

	#FeaturesSanitize: {
	}

	#FeaturesShape: {
	}

	#FeaturesSend: {
		_args: {
			egress_method: string
			kind:          string
		}
		let Args = _args

		if Args.egress_method == "batch" || Args.egress_method == "dynamic" {
			// `batch` describes how the component batches data. This is only
			// relevant if a component has an `egress_method` of "batch".
			batch: close({
				enabled:      bool
				common:       bool
				max_bytes:    uint | null
				max_events:   uint | null
				timeout_secs: uint16 | null
			})
		}

		// `compression` describes how the component compresses data.
		compression: {
			enabled: bool

			if enabled == true {
				default: #CompressionAlgorithm
				algorithms: [#CompressionAlgorithm, ...#CompressionAlgorithm]
				levels: [#CompressionLevel, ...#CompressionLevel]
			}
		}

		// `encoding` describes how the component encodes data.
		encoding: {
			enabled: bool

			if enabled {
				codec: {
					enabled: bool

					if enabled {
						default: #EncodingCodec | null
						enum:    [#EncodingCodec, ...#EncodingCodec] | null
					}
				}
			}
		}

		send_buffer_bytes?: #FeaturesSendBufferBytes

		keepalive?: #FeaturesKeepalive

		// `request` describes how the component issues and manages external
		// requests.
		request: {
			enabled: bool

			if enabled {
				adaptive_concurrency:       bool | *true
				concurrency:                uint8 | *5
				rate_limit_duration_secs:   uint8
				rate_limit_num:             uint16
				retry_initial_backoff_secs: uint8
				retry_max_duration_secs:    uint8
				timeout_secs:               uint8
				headers:                    bool
			}
		}

		// `tls` describes if the component secures network communication
		// via TLS.
		tls: #FeaturesTLS & {_args: {mode: "connect"}}

		to?: {
			service:    #Service
			interface?: #Interface
		}
	}

	#FeaturesTLS: {
		_args: {
			mode: "accept" | "connect"
		}
		let Args = _args
		enabled: bool

		if enabled {
			can_enable:             bool
			can_verify_certificate: bool
			if Args.mode == "connect" {
				can_verify_hostname: bool
			}
			enabled_default: bool
		}
	}

	#Input: {
		logs:    bool
		metrics: #MetricInput | null
	}

	#LogOutput: [Name=string]: close({
		description: string
		name:        Name
		fields:      #Schema
	})

	#MetricInput: {
		counter:      *false | bool
		distribution: *false | bool
		gauge:        *false | bool
		histogram:    *false | bool
		set:          *false | bool
		summary:      *false | bool
	}

	#MetricOutput: [Name=string]: close({
		description:       string
		relevant_when?:    string
		tags:              #MetricTags
		name:              Name
		type:              #MetricType
		default_namespace: string
	})

	#Output: {
		logs?:    #LogOutput
		metrics?: #MetricOutput
	}

	#IAM: {
		#Policy: {
			#RequiredFor: "write" | "healthcheck"

			_action:        !=""
			required_for:   *["write"] | [#RequiredFor, ...#RequiredFor]
			docs_url:       !=""
			required_when?: !=""

			if platform == "aws" {
				docs_url: "https://docs.aws.amazon.com/\(_docs_tag)/latest/APIReference/API_\(_action).html"
				action:   "\(_service):\(_action)"
			}
			if platform == "gcp" {
				docs_url: "https://cloud.google.com/iam/docs/permissions-reference"
				action:   "\(_service).\(_action)"
			}
		}

		platform: "aws" | "gcp"
		policies: [#Policy, ...#Policy]
		_service: !="" // The slug of the service, e.g. "s3" or "firehose"
		// _docs_tag is used to ed to construct URLs, e.g. "AmazonCloudWatchLogs" in
		// https://docs.aws.amazon.com/AmazonCloudWatchLogs/latest/APIReference/API_DescribeLogStreams.html
		_docs_tag: *_service | !=""
	}

	#Runtime: {
		name:    string
		url:     string
		version: string | null
	}

	#Support: {
		_args: kind: string

		// `requirements` describes any external requirements that the component
		// needs to function properly.
		//
		// For example, the `journald` source requires the presence of the
		// `journalctl` binary.
		requirements: [...string] | null // Allow for empty list

		// `targets` describes which targets this component is available on.
		targets: #TargetTriples

		// `warnings` describes any warnings the user should know about the
		// component.
		//
		// For example, the `grok_parser` might offer a performance warning
		// since the `regex_parser` and other transforms are faster.
		warnings: [...string] | null // Allow for empty list

		// `notices` communicates useful information to the user that is neither
		// a requirement or a warning.
		//
		// For example, the `lua` transform offers a Lua version notice that
		// communicate which version of Lua is embedded.
		notices: [...string] | null // Allow for empty list
	}

	sources:    #Components
	transforms: #Components
	sinks:      #Components

	{[Kind=string]: [Name=string]: {
		kind: string
		let Kind = kind

		classes: #Classes & {_args: kind: Kind}

		configuration: {
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
							syntax: "literal"
						}
					}
					crt_file: {
						common:      false
						description: "Absolute path to a certificate file used to identify this server, in DER or PEM format (X.509) or PKCS#12, or an in-line certificate in PEM format. If this is set, and is not a PKCS#12 archive, `key_file` must also be set. This is required if `enabled` is set to `true`."
						required:    false
						type: string: {
							default: null
							examples: ["/path/to/host_certificate.crt"]
							syntax: "literal"
						}
					}
					key_file: {
						common:      false
						description: "Absolute path to a private key file used to identify this server, in DER or PEM format (PKCS#8), or an in-line private key in PEM format."
						required:    false
						type: string: {
							default: null
							examples: ["/path/to/host_certificate.key"]
							syntax: "literal"
						}
					}
					key_pass: {
						common:      false
						description: "Pass phrase used to unlock the encrypted key file. This has no effect unless `key_file` is set."
						required:    false
						type: string: {
							default: null
							examples: ["${KEY_PASS_ENV_VAR}", "PassWord1"]
							syntax: "literal"
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
							syntax: "literal"
						}
					}
					crt_file: {
						common:      true
						description: "Absolute path to a certificate file used to identify this connection, in DER or PEM format (X.509) or PKCS#12, or an inline certificate in PEM format. If this is set and is not a PKCS#12 archive, `key_file` must also be set."
						required:    false
						type: string: {
							default: null
							examples: ["/path/to/host_certificate.crt"]
							syntax: "literal"
						}
					}
					key_file: {
						common:      true
						description: "Absolute path to a private key file used to identify this connection, in DER or PEM format (PKCS#8), or an inline private key in PEM format. If this is set, `crt_file` must also be set."
						required:    false
						type: string: {
							default: null
							examples: ["/path/to/host_certificate.key"]
							syntax: "literal"
						}
					}
					key_pass: {
						common:      false
						description: "Pass phrase used to unlock the encrypted key file. This has no effect unless `key_file` is set."
						required:    false
						type: string: {
							default: null
							examples: ["${KEY_PASS_ENV_VAR}", "PassWord1"]
							syntax: "literal"
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
							syntax: "literal"
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
							syntax: "literal"
						}
					}
					token: {
						description: "The token to use for bearer authentication"
						required:    true
						warnings: []
						type: string: {
							examples: ["${API_TOKEN}", "xyz123"]
							syntax: "literal"
						}
					}
					user: {
						description: "The basic authentication user name."
						required:    true
						warnings: []
						type: string: {
							examples: [Args.username_example, "username"]
							syntax: "literal"
						}
					}
				}
			}

			_http_basic_auth: {
				common:      false
				description: "Options for HTTP Basic Authentication."
				required:    false
				warnings: []
				type: object: {
					examples: []
					options: {
						username: {
							description: "The basic authentication user name."
							required:    true
							warnings: []
							type: string: {
								examples: ["${HTTP_USERNAME}", "username"]
								syntax: "literal"
							}
						}
						password: {
							description: "The basic authentication password."
							required:    true
							warnings: []
							type: string: {
								examples: ["${HTTP_PASSWORD}", "password"]
								syntax: "literal"
							}
						}
					}
				}
			}

			_types: {
				common:      true
				description: """
					Key/value pairs representing mapped log field names and types. This is used to
					coerce log fields from strings into their proper types. The available types are
					listed in the **Types** list below.

					Timestamp coercions need to be prefaced with `timestamp|`, for example
					`\"timestamp|%F\"`. Timestamp specifiers can use either of the following:

					1. One of the built-in-formats listed in the **Timestamp Formats** table below.
					2. The [time format specifiers](\(urls.chrono_time_formats)) from Rust's
					`chrono` library.

					### Types

					* `array`
					* `bool`
					* `bytes`
					* `float`
					* `int`
					* `map`
					* `null`
					* `timestamp` (see the table below for formats)

					### Timestamp Formats

					Format | Description | Example
					:------|:------------|:-------
					`%F %T` | `YYYY-MM-DD HH:MM:SS` | `2020-12-01 02:37:54`
					`%v %T` | `DD-Mmm-YYYY HH:MM:SS` | `01-Dec-2020 02:37:54`
					`%FT%T` | [ISO 8601](\(urls.iso_8601))\\[RFC 3339](\(urls.rfc_3339)) format without time zone | `2020-12-01T02:37:54`
					`%a, %d %b %Y %T` | [RFC 822](\(urls.rfc_822))/[2822](\(urls.rfc_2822)) without time zone | `Tue, 01 Dec 2020 02:37:54`
					`%a %d %b %T %Y` | [`date`](\(urls.date)) command output without time zone | `Tue 01 Dec 02:37:54 2020`
					`%a %b %e %T %Y` | [ctime](\(urls.ctime)) format | `Tue Dec  1 02:37:54 2020`
					`%s` | [UNIX](\(urls.unix_timestamp)) timestamp | `1606790274`
					`%FT%TZ` | [ISO 8601](\(urls.iso_8601))/[RFC 3339](\(urls.rfc_3339)) UTC | `2020-12-01T09:37:54Z`
					`%+` | [ISO 8601](\(urls.iso_8601))/[RFC 3339](\(urls.rfc_3339)) UTC with time zone | `2020-12-01T02:37:54-07:00`
					`%a %d %b %T %Z %Y` | [`date`](\(urls.date)) command output with time zone | `Tue 01 Dec 02:37:54 PST 2020`
					`%a %d %b %T %z %Y`| [`date`](\(urls.date)) command output with numeric time zone | `Tue 01 Dec 02:37:54 -0700 2020`
					`%a %d %b %T %#z %Y` | [`date`](\(urls.date)) command output with numeric time zone (minutes can be missing or present) | `Tue 01 Dec 02:37:54 -07 2020`

					**Note**: the examples in this table are for 54 seconds after 2:37 am on December 1st, 2020 in Pacific Standard Time.
					"""
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
							timestamp_unix:    "timestamp|%F %T"
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
					type: array: items: type: string: {
						examples: ["my-source-or-transform-id"]
						syntax: "literal"
					}
				}
			}

			"type": {
				description: "The component type. This is a required field for all components and tells Vector which component to use."
				required:    true
				sort:        -2
				"type": string: {
					enum: #Enum | *{
						"\(Name)": "The type of this component."
					}
					syntax: "literal"
				}
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
						collect_context: "Enriches data with useful \(features.collect.from.service.name) context."
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
						receive_context: "Enriches data with useful \(features.receive.from.service.name) context."
					}

					if features.receive.keepalive.enabled != _|_ {
						keepalive: "Supports TCP keepalive for efficient resource use and reliability."
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

					if features.send.keepalive.enabled != _|_ {
						keepalive: "Supports TCP keepalive for efficient resource use and reliability."
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
					type:              "counter"
					default_namespace: "vector"
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
					type:              "distribution"
					default_namespace: "vector"
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
					type:              "gauge"
					default_namespace: "vector"
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
					type:              "gauge"
					default_namespace: "vector"
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
					type:              "gauge"
					default_namespace: "vector"
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
					type:              "gauge"
					default_namespace: "vector"
				}
			}
		}

		if Kind == "transform" {
			telemetry: metrics: {
				// Default metrics for each transform
				processed_events_total: components.sources.internal_metrics.output.metrics.processed_events_total
				processed_bytes_total:  components.sources.internal_metrics.output.metrics.processed_bytes_total
			}
		}

		how_it_works: {
			state: {
				title: "State"

				if classes.stateful == true {
					body: """
						This component is stateful, meaning its behavior changes based on previous inputs (events).
						State is not preserved across restarts, therefore state-dependent behavior will reset between
						restarts and depend on the inputs (events) received since the most recent restart.
						"""
				}

				if classes.stateful == false {
					body: """
						This component is stateless, meaning its behavior is consistent across each input.
						"""
				}
			}
		}
	}}
}
