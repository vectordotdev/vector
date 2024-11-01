package metadata

components: sources: docker_logs: {
	title: "Docker"
	alias: "docker"

	classes: {
		commonly_used: false
		delivery:      "best_effort"
		deployment_roles: ["daemon"]
		development:   "stable"
		egress_method: "stream"
		stateful:      false
	}

	env_vars: {
		DOCKER_HOST: {
			description: "The Docker host to connect to when `docker_host` configuration is absent."
			type: string: {
				default: null
				examples: ["unix:///var/run/docker.sock"]
			}
		}

		DOCKER_CERT_PATH: {
			description: """
				Path to look for TLS certificates when `tls` configuration is absent.
				Vector will use:
				- `$DOCKER_CERT_PATH/ca.pem`: CA certificate.
				- `$DOCKER_CERT_PATH/cert.pem`: TLS certificate.
				- `$DOCKER_CERT_PATH/key.pem`: TLS key.
				"""
			type: string: {
				default: null
				examples: ["certs/"]
			}
		}

		DOCKER_CONFIG: {
			description: "Path to look for TLS certificates when both `tls` configuration and `DOCKER_CERT_PATH` are absent."
			type: string: {
				default: null
				examples: ["certs/"]
			}
		}
	}

	features: {
		acknowledgements: false
		auto_generated:   true
		collect: {
			checkpoint: enabled: false
			from: {
				service: services.docker

				interface: socket: {
					api: {
						title: "Docker Engine API"
						url:   urls.docker_engine_api
					}
					direction: "outgoing"
					permissions: unix: group: "docker"
					protocols: ["http"]
					socket: "/var/run/docker.sock"
					ssl:    "disabled"
				}
			}
		}
		multiline: enabled: true
	}

	support: {
		requirements: []
		warnings: [
			"""
				To avoid collecting logs from itself when deployed as a container,
				the Docker source uses current hostname to find out which container
				it is inside. If a container's ID matches the hostname, that container
				will be excluded.
				If you change container's hostname, consider manually excluding Vector
				container using [`exclude_containers`](#exclude_containers).
				""",
		]
		notices: []
	}

	installation: {
		platform_name: "docker"
	}

	configuration: base.components.sources.docker_logs.configuration

	output: logs: {
		log: {
			description: "A Docker log event"
			fields: {
				container_created_at: {
					description: "A UTC timestamp representing when the container was created."
					required:    true
					type: timestamp: {}
				}
				container_id: {
					description: "The Docker container ID that the log was collected from."
					required:    true
					type: string: {
						examples: ["9b6247364a03", "715ebfcee040"]
					}
				}
				container_name: {
					description: "The Docker container name that the log was collected from."
					required:    true
					type: string: {
						examples: ["evil_ptolemy", "nostalgic_stallman"]
					}
				}
				image: {
					description: "The image name that the container is based on."
					required:    true
					type: string: {
						examples: ["ubuntu:latest", "busybox", "timberio/vector:latest-alpine"]
					}
				}
				message: {
					description: "The raw log message."
					required:    true
					type: string: {
						examples: ["Started GET / for 127.0.0.1 at 2012-03-10 14:28:14 +0100"]
					}
				}
				source_type: {
					description: "The name of the source type."
					required:    true
					type: string: {
						examples: ["docker"]
					}
				}
				stream: {
					description: "The [standard stream](\(urls.standard_streams)) that the log was collected from."
					required:    true
					type: string: {
						enum: {
							stdout: "The STDOUT stream"
							stderr: "The STDERR stream"
						}
					}
				}
				timestamp: {
					description: "The UTC timestamp extracted from the Docker log event."
					required:    true
					type: timestamp: {}
				}
				host: fields._local_host
				label: {
					description: "Each container label is inserted with it's exact key/value pair."
					required:    true
					type: object: {
						examples: [{"mylabel": "myvalue"}]
					}
				}
			}
		}
	}

	examples: [
		{
			_container_name: "flog"
			_image:          "mingrammer/flog"
			_message:        "150.75.72.205 - - [03/Oct/2020:16:11:29 +0000] \"HEAD /initiatives HTTP/1.1\" 504 117"
			_stream:         "stdout"
			title:           "Dummy Logs"
			configuration: {
				include_images: [_image]
			}
			input: """
				 ```json
				 {
				   "stream": "\(_stream)",
				   "message": "\(_message)"
				 }
				```
				"""
			output: log: {
				container_created_at: "2020-10-03T16:11:29.443232Z"
				container_id:         "fecc98177eca7fb75a2b2186c418bf9a0cd3a05a1169f2e2293bf8987a9d96ab"
				container_name:       _container_name
				image:                _image
				message:              _message
				stream:               _stream
				host:                 _values.local_host
				source_type:          "docker"
			}
		},
	]

	how_it_works: {
		message_merging: {
			title: "Merging Split Messages"
			body: """
				Docker, by default, splits log messages that exceed 16kb. This can be a
				rather frustrating problem because it produces malformed log messages that are
				difficult to work with. Vector's solves this by default, automatically merging
				these messages into a single message. You can turn this off via the
				`auto_partial_merge` option. Furthermore, you can adjust the marker
				that we use to determine if an event is partial via the
				`partial_event_marker_field` option.
				"""
		}
	}

	telemetry: metrics: {
		container_processed_events_total: components.sources.internal_metrics.output.metrics.container_processed_events_total
		containers_unwatched_total:       components.sources.internal_metrics.output.metrics.containers_unwatched_total
		containers_watched_total:         components.sources.internal_metrics.output.metrics.containers_watched_total
	}
}
