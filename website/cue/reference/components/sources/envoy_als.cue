package metadata

components: sources: envoy_als: {
	_grpc_port: 9999

	title: "Envoy ALS"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["daemon", "aggregator"]
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		acknowledgements: false
		multiline: enabled: false
		receive: {
			from: {
				service: services.envoy_als

				interface: socket: {
					direction: "incoming"
					port:      _grpc_port
					protocols: ["tcp"]
					ssl: "optional"
				}
			}
			tls: {
				// enabled per listener below
				enabled: false
			}
		}
	}

	support: {
		requirements: []
		warnings: [
			"""
				The `envoy_als` source only supports HTTP log events at this time. TCP logs will not be ingested
				and will generate a warning in Vector logs if sent. Please make sure you are using HTTP access logs in
				you Envoy configuration.
				""",
		]
		notices: [
			"""
			The envoy protos were generated from version v1.24.1. Results may vary if using a different version.
			"""
		]
	}

	installation: {
		platform_name: null
	}

	configuration: {
		grpc: {
			description: "Configuration options for the gRPC server."
			required:    true
			type: object: {
				examples: [{address: "0.0.0.0:\(_grpc_port)"}]
				options: {
					address: {
						description: """
						The gRPC address to listen for connections on. It _must_ include a port.
						"""
						required: true
						type: string: {
							examples: ["0.0.0.0:\(_grpc_port)"]
						}
					}
					tls: configuration._tls_accept & {_args: {
						can_verify_certificate: true
						enabled_default:        false
					}}
				}
			}
		}
	}

	configuration_examples: [
		{
			title: "Envoy ALS Defaults"
			configuration: {
				envoy_als: {
					grpc: {
						address: "0.0.0.0:\(_grpc_port)"
						tls: {
							enabled:  true
							crt_file: "/etc/ssl/certs/vector.pem"
							key_file: "/etc/ssl/private/vector.key"
						}
					}
				}
			}
		},
	]

	output: {
		logs: event: {
			description: "An individual log event from a batch of events received by an Envoy ALS stream. See https://www.envoyproxy.io/docs/envoy/v1.24.1/api-v3/service/accesslog/v3/als.proto#service-accesslog-v3-streamaccesslogsmessage for more details."
			fields: {
				"identifier.log_name": {
					description: "The name of the log configured in the grpc access log config."
					required:    false
					common:      true
					type: string: {
						default: ""
						examples: ["my-envoy-als"]
					}
				},
				"identifier.node.client_features": {
					description: "Well known features described in the Envoy API repository for a major version of an API"
					required:    false
					common:      false
					type: array: items: type: string: {
						examples: ["xds.config.supports-resource-ttl"]
					}
				},
				"identifier.node.cluster": {
					description: "Cluster name where envoy is running."
					required:    false
					common:      false
					type: string: {
						default: ""
						examples: ["my-cluster"]
					}
				},
				"identifier.node.dynamic_parameters": {
					description: "Map from xDS resource type URL to dynamic context parameters."
					required:    false
					common:      false
					type: object: {
						examples: [
							{
								"xds.resource.listening_address": {
									"ip:port": "10.10.10.10:8080"
								}
							}
						]
					}
				},
				"identifier.node.id": {
					description: "An opaque node identifier for the Envoy node."
					required:    false
					common:      true
					type: string: {
						default: ""
						examples: ["my-node-name"]
					}
				},
				"identifier.node.user_agent_name": {
					description: "Name of the requesting entity"
					required:    false
					common:      false
					type: string: {
						default: ""
						examples: ["envoy"]
					}
				},
				"identifier.node.user_agent_version_type.user_agent_version": {
					description: "Version of the requesting entity. Only one of `user_agent_version` and `user_agent_build_version` will be set."
					required:    false
					common:      false
					type: string: {
						default: null
						examples: ["1.24.1"]
					}
				},
				"identifier.node.user_agent_version_type.user_agent_build_version": {
					description: "Envoy build information. Only one of `user_agent_version` and `user_agent_build_version` will be set."
					required:    false
					common:      false
					type: object: {
						examples: [
							{
								"metadata": {
									"build.type": "RELEASE",
									"revision.sha": "69958e4fe32da561376d8b1d367b5e6942dfba24",
									"revision.status": "Clean",
									"ssl.version": "BoringSSL"
								},
								"version": {
									"major_number": 1,
									"minor_number": 24,
									"patch": 1,
								}
							}
						]
					}
				},
				"common_properties.downstream_remote_address": {
					description: "Remote/origin address of the client request. One of `socket_address`, `pipe`, and `envoy_internal_address` will be set. See https://www.envoyproxy.io/docs/envoy/v1.24.1/api-v3/config/core/v3/address.proto#config-core-v3-address for more details."
					required:    false
					common:      false
					type: object: {
						examples: [
							{
								"socket_address": {
								  "protocol": "TCP",
								  "address": "10.10.10.1",
								  "port_value": 45952,
								  "named_port": "",
								  "resolver_name": "",
								  "ipv4_compat": false
								}
							},
							{
								"pipe": {
									"path": "/my/pipe.sock"
									"mode": 438
								}
							},
							{
								"envoy_internal_address": {
									"endpoint_id": "",
									"address_name_specifier": {
										"server_listener_name": "my-internal-listener"
									}
								}
							},
						]
					}
				},
				"common_properties.downstream_local_address": {
					description: "Local/dest address of the client request. One of `socket_address`, `pipe`, and `envoy_internal_address` will be set. See https://www.envoyproxy.io/docs/envoy/v1.24.1/api-v3/config/core/v3/address.proto#config-core-v3-address for more details."
					required:    false
					common:      false
					type: object: {
						examples: [
							{
								"socket_address": {
								  "protocol": "TCP",
								  "address": "10.10.10.10",
								  "port_value": 8080,
								  "named_port": "",
								  "resolver_name": "",
								  "ipv4_compat": false
								}
							},
							{
								"pipe": {
									"path": "/my/pipe.sock"
									"mode": 438
								}
							},
							{
								"envoy_internal_address": {
									"endpoint_id": "",
									"address_name_specifier": {
										"server_listener_name": "my-internal-listener"
									}
								}
							},
						]
					}
				},
				"common_properties.tls_properties": {
					description: "Contains TLS info if the connection is secure. See https://www.envoyproxy.io/docs/envoy/v1.24.1/api-v3/data/accesslog/v3/accesslog.proto#data-accesslog-v3-tlsproperties for more details."
					required:    false
					common:      false
					type: object: {
						examples: [
							{
							  "tls_version": "TLSv1_3",
							  "tls_cipher_suite": 4865,
							  "tls_sni_hostname": "www.example.com",
							  "local_certificate_properties": {
							  	"subject_alt_name":  [
							  		{
							  			"san": {
											"uri": "example.com"
							  			}
							  		}
							  	],
							  	"subject": "CN=www.example.com"
							  },
							  "tls_session_id": "",
							  "ja3_fingerprint": "",
							}
						]
					}
				},
				"common_properties.start_time": {
					description: "The time Envoy received the first byte from the client."
					required:    false
					common:      false
					type: timestamp: {
						examples: ["2020-10-10T17:07:36.452332Z"]
					}
				},
				"common_properties.time_to_last_rx_byte": {
					description: "Nanoseconds between first and last byte from the client."
					required:    false
					common:      false
					type: uint: {
						examples: [50000000]
					}
				},
				"common_properties.time_to_first_upstream_tx_byte": {
					description: "Nanoseconds between first client byte received and first byte sent to the backend."
					required:    false
					common:      false
					type: uint: {
						examples: [5000000]
					}
				},
				"common_properties.time_to_last_upstream_tx_byte": {
					description: "Nanoseconds between first client byte received and last byte sent to the backend."
					required:    false
					common:      false
					type: uint: {
						examples: [60000000]
					}
				},
				"common_properties.time_to_first_upstream_rx_byte": {
					description: "Nanoseconds between first client byte received and first backend response byte recieved."
					required:    false
					common:      false
					type: uint: {
						examples: [65000000]
					}
				},
				"common_properties.time_to_last_upstream_rx_byte": {
					description: "Nanoseconds between first client byte received and last backend response byte recieved."
					required:    false
					common:      false
					type: uint: {
						examples: [70000000]
					}
				},
				"common_properties.time_to_first_downstream_tx_byte": {
					description: "Nanoseconds between first client byte received and first byte sent to the client."
					required:    false
					common:      false
					type: uint: {
						examples: [80000000]
					}
				},
				"common_properties.time_to_last_downstream_tx_byte": {
					description: "Nanoseconds between first client byte received and last byte sent to the client."
					required:    false
					common:      false
					type: uint: {
						examples: [900000000]
					}
				},
				"common_properties.upstream_remote_address": {
					description: "Remote/dest address of the backend request. One of `socket_address`, `pipe`, and `envoy_internal_address` will be set. See https://www.envoyproxy.io/docs/envoy/v1.24.1/api-v3/config/core/v3/address.proto#config-core-v3-address for more details."
					required:    false
					common:      false
					type: object: {
						examples: [
							{
								"socket_address": {
								  "protocol": "TCP",
								  "address": "10.10.10.12",
								  "port_value": 8080,
								  "named_port": "",
								  "resolver_name": "",
								  "ipv4_compat": false
								}
							},
							{
								"pipe": {
									"path": "/my/pipe.sock"
									"mode": 438
								}
							},
							{
								"envoy_internal_address": {
									"endpoint_id": "",
									"address_name_specifier": {
										"server_listener_name": "my-internal-listener"
									}
								}
							},
						]
					}
				},
				"common_properties.upstream_local_address": {
					description: "Local/origin address of the backend request. One of `socket_address`, `pipe`, and `envoy_internal_address` will be set. See https://www.envoyproxy.io/docs/envoy/v1.24.1/api-v3/config/core/v3/address.proto#config-core-v3-address for more details."
					required:    false
					common:      false
					type: object: {
						examples: [
							{
								"socket_address": {
								  "protocol": "TCP",
								  "address": "10.10.10.10",
								  "port_value": 34578,
								  "named_port": "",
								  "resolver_name": "",
								  "ipv4_compat": false
								}
							},
							{
								"pipe": {
									"path": "/my/pipe.sock"
									"mode": 438
								}
							},
							{
								"envoy_internal_address": {
									"endpoint_id": "",
									"address_name_specifier": {
										"server_listener_name": "my-internal-listener"
									}
								}
							},
						]
					}
				},
				"common_properties.upstream_cluster": {
					description: "The cluster the backend belongs to."
					required:    false
					common:      false
					type: string: {
						examples: ["my-cluster"]
					}
				},
				"common_properties.response_flags": {
					description: "Indicators for the request/response. See https://www.envoyproxy.io/docs/envoy/v1.24.1/api-v3/data/accesslog/v3/accesslog.proto#envoy-v3-api-msg-data-accesslog-v3-responseflags for more details."
					required:    false
					common:      false
					type: object: {
						examples: [
							{
							    "failed_local_healthcheck": false,
							    "no_healthy_upstream": false,
							    "upstream_request_timeout": false,
							    "local_reset": false,
							    "upstream_remote_reset": false,
							    "upstream_connection_failure": false,
							    "upstream_connection_termination": false,
							    "upstream_overflow": false,
							    "no_route_found": false,
							    "delay_injected": false,
							    "fault_injected": false,
							    "rate_limited": false,
							    "unauthorized_details": {
							  	    "reason": "REASON_UNSPECIFIED"
							    },
							    "rate_limit_service_error": false,
							    "downstream_connection_termination": false,
							    "upstream_retry_limit_exceeded": false,
							    "stream_idle_timeout": false,
							    "invalid_envoy_request_headers": false,
							    "downstream_protocol_error": false,
							    "upstream_max_stream_duration_reached": false,
							    "response_from_cache_filter": false,
							    "no_filter_config_found": false,
							    "duration_timeout": false,
							    "upstream_protocol_error": false,
							    "no_cluster_found": false,
							    "overload_manager": false,
							    "dns_resolution_failure": false
							}
						]
					}
				},
				"common_properties.metadata": {
					description: "Metadata encountered during the request. See https://www.envoyproxy.io/docs/envoy/v1.24.1/api-v3/config/core/v3/base.proto#config-core-v3-metadata for more details."
					required:    false
					common:      false
					type: object: {
						examples: [
							{
								"filter_metadata": {
									"envoy.filters.http.cdn": {
										"cached": true
									}
								}
							}
						]
					}
				},
				"common_properties.upstream_transport_failure_reason": {
					description: "If the backend connection failed due to the transport socket, this provides the failure reason."
					required:    false
					common:      false
					type: string: {
						examples: ["SSLV3_ALERT_CERTIFICATE_EXPIRED"]
					}
				},
				"common_properties.route_name": {
					description: "Name of the route."
					required:    false
					common:      false
					type: string: {
						examples: ["my-route"]
					}
				},
				"common_properties.downstream_direct_remote_address": {
					description: "The unedited client address. For example, this is not affected by the `x-forwarded-for` header or proxy protocol."
					required:    false
					common:      false
					type: object: {
						examples: [
							{
								"socket_address": {
								  "protocol": "TCP",
								  "address": "10.10.10.1",
								  "port_value": 45952,
								  "named_port": "",
								  "resolver_name": "",
								  "ipv4_compat": false
								}
							},
							{
								"pipe": {
									"path": "/my/pipe.sock"
									"mode": 438
								}
							},
							{
								"envoy_internal_address": {
									"endpoint_id": "",
									"address_name_specifier": {
										"server_listener_name": "my-internal-listener"
									}
								}
							},
						]
					}
				},
				"common_properties.filter_state_objects": {
					description: "Map of filter state in stream info that have been configured to be logged."
					required:    false
					common:      false
					type: object: {
						examples: [
							{
								"my-object": {
									"type_url": "type.googleapis.com/google.protobuf.Duration",
									"value": "0.8s"
								}
							}
						]
					}
				},
				"common_properties.custom_tags": {
					description: "A map of custom tags configured to be logged in Envoy."
					required:    false
					common:      false
					type: object: {
						examples: [
							{
								"my-tag": "my-val"
							}
						]
					}
				},
				"common_properties.duration": {
					description: "For HTTP, total duration of the request in nanosecoconds."
					required:    false
					common:      false
					type: uint: {
						examples: [900000000]
					}
				},
				"common_properties.upstream_request_attempt_count": {
					description: "For HTTP, the number of times the request was attempted upstream."
					required:    false
					common:      false
					type: uint: {
						examples: [1]
					}
				},
				"common_properties.connection_termination_details": {
					description: "This may provide additional information about why the connection was terminated for L4 reasons."
					required:    false
					common:      false
					type: string: {
						examples: ["access_denied"]
					}
				},
				"protocol_version": {
					description: "HTTP version of the request."
					required:    false
					common:      false
					type: string: {
						examples: ["HTTP2"]
					}
				},
				"request.request_method": {
					description: "The request method (RFC 7231/2616)."
					required:    false
					common:      false
					type: string: {
						examples: ["GET"]
					}
				},
				"request.scheme": {
					description: "The URI scheme."
					required:    false
					common:      false
					type: string: {
						examples: ["https"]
					}
				},
				"request.authority": {
					description: "The HTTP/2 `:authority` or HTTP/1.1 `host` header value."
					required:    false
					common:      false
					type: string: {
						examples: ["www.example.com"]
					}
				},
				"request.path": {
					description: "The request path."
					required:    false
					common:      false
					type: string: {
						examples: ["/"]
					}
				},
				"request.user_agent": {
					description: "The request `user-agent` header."
					required:    false
					common:      false
					type: string: {
						examples: ["Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/105.0.0.0 Safari/537.36"]
					}
				},
				"request.referer": {
					description: "The request `referer` header."
					required:    false
					common:      false
					type: string: {
						examples: ["https://example.com/"]
					}
				},
				"request.forwarded_for": {
					description: "The request `x-forwarded-for` header."
					required:    false
					common:      false
					type: string: {
						examples: ["203.0.113.195"]
					}
				},
				"request.request_id": {
					description: "The request `x-request-id` header."
					required:    false
					common:      false
					type: string: {
						examples: ["b41a828c-873b-4649-9b6e-a17b9af05c5a"]
					}
				},
				"request.original_path": {
					description: "The request `x-envoy-original-path` header."
					required:    false
					common:      false
					type: string: {
						examples: ["/"]
					}
				},
				"request.request_headers_bytes": {
					description: "The size of the request headers in bytes."
					required:    false
					common:      false
					type: uint: {
						examples: [300]
					}
				},
				"request.request_body_bytes": {
					description: "The size of the request body in bytes."
					required:    false
					common:      false
					type: uint: {
						examples: [1200]
					}
				},
				"request.request_headers": {
					description: "Additional request headers that have been configured for logging."
					required:    false
					common:      false
					type: object: {
						examples: [{
							"my-customer-header": "my-value"
						}]
					}
				},
				"response.response_code": {
					description: "The HTTP response code sent by Envoy."
					required:    false
					common:      false
					type: uint: {
						examples: [200]
					}
				},
				"response.response_headers_bytes": {
					description: "The size of the response headers in bytes."
					required:    false
					common:      false
					type: uint: {
						examples: [200]
					}
				},
				"response.response_body_bytes": {
					description: "The size of the response body in bytes."
					required:    false
					common:      false
					type: uint: {
						examples: [800]
					}
				},
				"response.response_headers": {
					description: "Response headers that have been configured for logging."
					required:    false
					common:      false
					type: object: {
						examples: [{
							"content-type": "text/html; charset=utf-8"
						}]
					}
				},
				"response.response_trailers": {
					description: "Response trailers that have been configured for logging."
					required:    false
					common:      false
					type: object: {
						examples: [{
							"expires": "Wed, 15 Feb 2023 08:00:00 GMT"
						}]
					}
				},
				"response.response_code_details": {
					description: "Additional request headers that have been configured for logging."
					required:    false
					common:      false
					type: string: {
						examples: ["via_upstream"]
					}
				}
			}
		}
	}

	how_it_works: {
		tls: {
			title: "Envoy ALS"
			body:  """
				In your Envoy configuration, create an `access_log` entry that points to the running Vector
				instance with the Envoy ALS source configured. Envoy will then stream logs to Vector on the
				interval configured in Envoy.
				"""
		}
	}
}
