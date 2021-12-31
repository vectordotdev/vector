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

		// `description` describes the components with a single paragraph. It
		// should be 1-3 sentences.  It is used for SEO purposes and should be
		// full of relevant keywords.
		description?: =~"[.]$"

		env_vars: #EnvVars

		// `alias` is used to register a component's former name when it
		// undergoes a name change.
		alias?: !=""

		// `type` is the component type. This is set automatically.
		type: string

		// `classes` represent the various classifications for this component
		classes: #Classes & {_args: kind: Kind}

		// `examples` demonstrates various ways to use the component using an
		// input, output, and example configuration.
		#ExampleConfig: {
			title:           string
			context?:        string
			"configuration": {
				...
				for k, v in configuration {
					"\( k )"?: _ | *null
				}
			} | string

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
		}

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
			codecs?:   #FeaturesCodecs
			encoding?: #FeaturesEncoding
			receive?:  #FeaturesReceive
		}

		if Args.kind == "transform" {
			aggregate?: #FeaturesAggregate
			convert?:   #FeaturesConvert
			enrich?:    #FeaturesEnrich
			filter?:    #FeaturesFilter
			parse?:     #FeaturesParse
			program?:   #FeaturesProgram
			proxy?:     #FeaturesProxy
			reduce?:    #FeaturesReduce
			route?:     #FeaturesRoute
			sanitize?:  #FeaturesSanitize
			shape?:     #FeaturesShape
		}

		if Args.kind == "sink" {
			// `buffer` describes how the component buffers data.
			buffer: {
				enabled: bool | string
			}

			// `healtcheck` notes if a component offers a healthcheck on boot.
			healthcheck: {
				enabled: bool
			}

			exposes?: #FeaturesExpose
			send?:    #FeaturesSend & {_args: Args}
		}

		descriptions: [Name=string]: string
	}

	#FeaturesAggregate: {
	}

	#FeaturesCollect: {
		checkpoint: {
			enabled: bool
		}

		from?: {
			service:    #Service
			interface?: #Interface
		}

		proxy?: #FeaturesProxy

		tls?: #FeaturesTLS & {_args: {mode: "connect"}}
	}

	#FeaturesConvert: {
	}

	#FeaturesEnrich: {
		from: service: {
			name:     string
			url:      string
			versions: string | null
		}
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

	#FeaturesCodecs: {
		enabled:         bool
		default_framing: string
	}

	#FeaturesEncoding: {
		enabled: bool
	}

	#FeaturesParse: {
		format: {
			name:     string
			url:      string | null
			versions: string | null
		}
	}

	#FeaturesProgram: {
		runtime: #Runtime
	}

	#FeaturesProxy: {
		enabled: bool
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
			batch: {
				enabled:      bool
				common:       bool
				max_bytes?:   uint | null
				max_events?:  uint | null
				timeout_secs: uint16 | null
			}
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
						batched: bool | *false
						enum:    [#EncodingCodec, ...#EncodingCodec] | null
					}
				}
			}
		}

		send_buffer_bytes?: #FeaturesSendBufferBytes

		keepalive?: #FeaturesKeepalive

		proxy?: #FeaturesProxy

		// `request` describes how the component issues and manages external
		// requests.
		request: {
			enabled: bool

			if enabled {
				adaptive_concurrency:       bool | *true
				concurrency:                uint64 | *null
				rate_limit_duration_secs:   uint64 | *1
				rate_limit_num:             uint64 | *9223372036854775807
				retry_initial_backoff_secs: uint64 | *1
				retry_max_duration_secs:    uint64 | *3600
				timeout_secs:               uint64 | *60
				headers:                    bool
				relevant_when?:             string
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

	#LogOutput: [Name=string]: {
		description: string
		name:        Name
		fields:      #Schema
	}

	#MetricInput: {
		counter:      *false | bool
		distribution: *false | bool
		gauge:        *false | bool
		histogram:    *false | bool
		set:          *false | bool
		summary:      *false | bool
	}

	#MetricOutput: [Name=string]: {
		description:       string
		relevant_when?:    string
		tags:              #MetricTags
		name:              Name
		type:              #MetricType
		default_namespace: string
	}

	#Output: {
		logs?:    #LogOutput
		metrics?: #MetricOutput
	}

	#IAM: {
		#Policy: {
			#RequiredFor: "operation" | "healthcheck"

			// TODO: come up with a less janky URL generation scheme
			_action:        !=""
			required_for:   *["operation"] | [#RequiredFor, ...#RequiredFor]
			docs_url:       !=""
			required_when?: !=""

			if platform == "aws" {
				docs_url: "https://docs.aws.amazon.com/\(_docs_tag)/latest/\(_url_fragment)/API_\(_action).html"
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
		_docs_tag:     *_service | !=""
		_url_fragment: !="" | *"APIReference"

		// For use in the view layer
		platform_title: !=""
		platform_link:  !=""

		if platform == "aws" {
			platform_title: "Amazon Web Services"
			platform_link:  "https://aws.amazon.com"
		}
		if platform == "gcp" {
			platform_title: "Google Cloud Platform"
			platform_link:  "https://cloud.google.com"
		}
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
			_acknowledgements: {
				common:      true
				description: "Controls how acknowledgements are handled by this source."
				required:    false
				type: object: options: {
					enabled: {
						common:      true
						description: "Controls if the source will wait for destination sinks to deliver the events before acknowledging receipt."
						warnings: ["Disabling this option may lead to loss of data, as destination sinks may reject events after the source acknowledges their successful receipt."]
						required: false
						type: bool: default: false
					}
				}
			}

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
				description: "Configures the TLS options for outgoing connections."
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
						}
					}
					crt_file: {
						common:      true
						description: "Absolute path to a certificate file used to identify this connection, in DER or PEM format (X.509) or PKCS#12, or an inline certificate in PEM format. If this is set and is not a PKCS#12 archive, `key_file` must also be set."
						required:    false
						type: string: {
							default: null
							examples: ["/path/to/host_certificate.crt"]
						}
					}
					key_file: {
						common:      true
						description: "Absolute path to a private key file used to identify this connection, in DER or PEM format (PKCS#8), or an inline private key in PEM format. If this is set, `crt_file` must also be set."
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

			_proxy: {
				common:      false
				description: "Configures an HTTP(S) proxy for Vector to use. By default, the globally configured proxy is used."
				required:    false
				type: object: options: {
					enabled: {
						common:      false
						description: "If false the proxy will be disabled."
						required:    false
						type: bool: default: true
					}
					http: {
						common:      false
						description: "The URL to proxy HTTP requests through."
						required:    false
						type: string: {
							default: null
							examples: ["http://foo.bar:3128"]
						}
					}
					https: {
						common:      false
						description: "The URL to proxy HTTPS requests through."
						required:    false
						type: string: {
							default: null
							examples: ["http://foo.bar:3128"]
						}
					}
					no_proxy: {
						common:      false
						description: """
							A list of hosts to avoid proxying. Allowed patterns here include:

							Pattern | Example match
							:-------|:-------------
							Domain names | `example.com` matches requests to `example.com`
							Wildcard domains | `.example.com` matches requests to `example.com` and its subdomains
							IP addresses | `127.0.0.1` matches requests to 127.0.0.1
							[CIDR](\(urls.cidr)) blocks | `192.168.0.0./16` matches requests to any IP addresses in this range
							Splat | `*` matches all hosts
							"""
						required:    false
						type: array: {
							default: null
							items: type: string: {
								examples: ["localhost", ".foo.bar", "*"]
							}
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
						type: string: {
							examples: [Args.password_example, "password"]
						}
					}
					strategy: {
						description: "The authentication strategy to use."
						required:    true
						type: string: {
							enum: {
								basic:  "The [basic authentication strategy](\(urls.basic_auth))."
								bearer: "The bearer token authentication strategy."
							}
						}
					}
					token: {
						description: "The token to use for bearer authentication"
						required:    true
						type: string: {
							examples: ["${API_TOKEN}", "xyz123"]
						}
					}
					user: {
						description: "The basic authentication user name."
						required:    true
						type: string: {
							examples: [Args.username_example, "username"]
						}
					}
				}
			}

			_http_basic_auth: {
				common:      false
				description: "Options for HTTP Basic Authentication."
				required:    false
				type: object: {
					examples: []
					options: {
						username: {
							description: "The basic authentication user name."
							required:    true
							type: string: {
								examples: ["${HTTP_USERNAME}", "username"]
							}
						}
						password: {
							description: "The basic authentication password."
							required:    true
							type: string: {
								examples: ["${HTTP_PASSWORD}", "password"]
							}
						}
					}
				}
			}

			_timezone: {
				common:      false
				description: """
					The name of the time zone to apply to timestamp conversions that do not contain an explicit time
					zone. This overrides the global [`timezone` option](\(urls.vector_configuration)/global-options#timezone).
					The time zone name may be any name in the [TZ database](\(urls.tz_time_zones)), or `local` to
					indicate system local time.
					"""
				required:    false
				type: string: {
					default: "local"
					examples: ["local", "America/NewYork", "EST5EDT"]
				}
			}

			_types: {
				common:      true
				description: _coercing_fields
				required:    false

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
					description: """
						A list of upstream [source](\(urls.vector_sources)) or [transform](\(urls.vector_transforms))
						IDs. Wildcards (`*`) are supported.

						See [configuration](\(urls.vector_configuration)) for more info.
						"""
					required:    true
					sort:        -1
					type: array: items: type: string: {
						examples: [
							"my-source-or-transform-id",
							"prefix-*",
						]
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
				}
			}
		}

		env_vars: {
			_http_proxy: {
				description: """
					The global URL to proxy HTTP requests through.

					If another HTTP proxy is set in the configuration file or at a component level,
					this one will be overridden.

					The lowercase variant has priority over the uppercase one.
					"""
				type: string: {
					default: null
					examples: ["http://foo.bar:3128"]
				}
			}
			_https_proxy: {
				description: """
					The global URL to proxy HTTPS requests through.

					If another HTTPS proxy is set in the configuration file or at a component level,
					this one will be overridden.

					The lowercase variant has priority over the uppercase one.
					"""
				type: string: {
					default: null
					examples: ["http://foo.bar:3128"]
				}
			}
			_no_proxy: {
				description: """
					List of hosts to avoid proxying globally.

					Allowed patterns here include:

					Pattern | Example match
					:-------|:-------------
					Domain names | `example.com` matches requests to `example.com`
					Wildcard domains | `.example.come` matches requests to `example.com` and its subdomains
					IP addresses | `127.0.0.1` matches requests to `127.0.0.1`
					[CIDR](\(urls.cidr)) blocks | `192.168.0.0./16` matches requests to any IP addresses in this range
					Splat | `*` matches all hosts

					If another `no_proxy` value is set in the configuration file or at a component level, this
					one is overridden.

					The lowercase variant has priority over the uppercase one.
					"""
				type: string: {
					default: null
					examples: ["localhost,.example.com,192.168.0.0./16", "*"]
				}
			}
			if features.collect != _|_ {
				if features.collect.proxy != _|_ {
					if features.collect.proxy.enabled {
						http_proxy:  env_vars._http_proxy
						HTTP_PROXY:  env_vars._http_proxy
						https_proxy: env_vars._https_proxy
						HTTPS_PROXY: env_vars._https_proxy
						no_proxy:    env_vars._no_proxy
						NO_PROXY:    env_vars._no_proxy
					}
				}
				if features.send.proxy != _|_ {
					if features.send.proxy.enabled {
						http_proxy:  env_vars._http_proxy
						HTTP_PROXY:  env_vars._http_proxy
						https_proxy: env_vars._https_proxy
						HTTPS_PROXY: env_vars._https_proxy
						no_proxy:    env_vars._no_proxy
						NO_PROXY:    env_vars._no_proxy
					}
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
