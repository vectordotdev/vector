package metadata

components: sources: docker_logs: {
	title:       "Docker"
	description: "Test."

	alias: "docker"

	classes: {
		commonly_used: false
		delivery:      "best_effort"
		deployment_roles: ["daemon"]
		development:   "stable"
		egress_method: "stream"
	}

	env_vars: {
		DOCKER_HOST: {
			description: "The Docker host to connect to."
			type: string: {
				default: null
				examples: ["unix:///var/run/docker.sock"]
			}
		}

		DOCKER_VERIFY_TLS: {
			description: "If `true` (the default), Vector will validate the TLS certificate of the remote host. Do NOT set this to `false` unless you understand the risks of not verifying the remote certificate."
			type: string: {
				default: "true"
				enum: {
					"true":  "true"
					"false": "false"
				}
			}
		}
	}

	features: {
		collect: {
			checkpoint: enabled: false
			from: {
				service: {
					name:     "Docker"
					thing:    "the \(name) platform"
					url:      urls.docker
					versions: ">= 1.24"

					setup: [
						"""
							Ensure that [Docker is setup](\(urls.docker_setup)) and running.
							""",
						"""
							Ensure that the Docker Engine is properly exposing logs:

							```bash
							docker logs $(docker ps | awk '{ print $1 }')
							```

							If you receive an error it's likely that you do not have
							the proper Docker logging drivers installed. The Docker
							Engine requires the [`json-file`](\(urls.docker_logging_driver_json_file)) (default),
							[`journald`](docker_logging_driver_journald), or [`local`](\(urls.docker_logging_driver_local)) Docker
							logging drivers to be installed.
							""",
					]
				}
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
		targets: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
			"x86_64-apple-darwin":        true
		}

		requirements: []
		warnings: [
			"""
				Collecting logs directly from the Docker Engine is known to have
				performance problems for very large setups. If you have a large
				setup, please consider alternative collection methods, such as the
				Docker [`syslog`](\(urls.docker_logging_driver_syslog)) or
				[Docker `journald` driver](\(urls.docker_logging_driver_journald))
				drivers.
				""",
		]
		notices: []
	}

	installation: {
		platform_name: "docker"
	}

	configuration: {
		auto_partial_merge: {
			common: false
			description: """
				Setting this to `false` will disable the automatic merging
				of partial events.
				"""
			required: false
			type: bool: default: true
		}
		include_containers: {
			common: true
			description: """
				A list of container IDs _or_ names to match against. Prefix
				matches are supported, meaning you can supply just the first
				few characters of the container ID or name. If not provided,
				all containers will be included.
				"""
			required: false
			type: array: {
				default: null
				items: type: string: examples: ["serene_", "serene_leakey", "ad08cc418cf9"]
			}
		}
		include_labels: {
			common:      true
			description: """
				A list of container object labels to match against when
				filtering running containers. This should follow the
				described label's synatx in [docker object labels docs](\(urls.docker_object_labels)).
				"""
			required:    false
			type: array: {
				default: null
				items: type: string: examples: ["com.example.vendor=Timber Inc.", "com.example.name=Vector"]
			}
		}
		include_images: {
			common: true
			description: """
				A list of image names to match against. If not provided, all
				images will be included.
				"""
			required: false
			type: array: {
				default: null
				items: type: string: examples: ["httpd", "redis"]
			}
		}
		retry_backoff_secs: {
			common: false
			description: """
				The amount of time to wait before retrying after an error.
				"""
			required: false
			type: uint: {
				unit:    "seconds"
				default: 1
			}
		}
	}

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
					type: string: examples: ["9b6247364a03", "715ebfcee040"]
				}
				container_name: {
					description: "The Docker container name that the log was collected from."
					required:    true
					type: string: examples: ["evil_ptolemy", "nostalgic_stallman"]
				}
				image: {
					description: "The image name that the container is based on."
					required:    true
					type: string: examples: ["ubuntu:latest", "busybox", "timberio/vector:latest-alpine"]
				}
				message: {
					description: "The raw log message."
					required:    true
					type: string: examples: ["Started GET / for 127.0.0.1 at 2012-03-10 14:28:14 +0100"]
				}
				stream: {
					description: "The [standard stream](\(urls.standard_streams)) that the log was collected from."
					required:    true
					type: string: enum: {
						stdout: "The STDOUT stream"
						stderr: "The STDERR stream"
					}
				}
				timestamp: {
					description: "The UTC timestamp extracted from the Docker log event."
					required:    true
					type: timestamp: {}
				}
				"*": {
					description: "Each container label is inserted with it's exact key/value pair."
					required:    true
					type: string: examples: ["Started GET / for 127.0.0.1 at 2012-03-10 14:28:14 +0100"]
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
			}
		},
	]

	how_it_works: {
		message_merging: {
			title: "Merging Split Messages"
			body: """
				Docker, by default, will split log messages that exceed 16kb. This can be a
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
		communication_errors_total:            components.sources.internal_metrics.output.metrics.communication_errors_total
		container_processed_events_total:      components.sources.internal_metrics.output.metrics.container_processed_events_total
		container_metadata_fetch_errors_total: components.sources.internal_metrics.output.metrics.container_metadata_fetch_errors_total
		containers_unwatched_total:            components.sources.internal_metrics.output.metrics.containers_unwatched_total
		containers_watched_total:              components.sources.internal_metrics.output.metrics.containers_watched_total
		logging_driver_errors_total:           components.sources.internal_metrics.output.metrics.logging_driver_errors_total
	}
}
