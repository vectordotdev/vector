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
				syntax: "literal"
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
				syntax: "literal"
			}
		}

		DOCKER_CONFIG: {
			description: "Path to look for TLS certificates when both `tls` configuration and `DOCKER_CERT_PATH` are absent."
			type: string: {
				default: null
				examples: ["certs/"]
				syntax: "literal"
			}
		}
	}

	features: {
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
		targets: {
			"aarch64-unknown-linux-gnu":      true
			"aarch64-unknown-linux-musl":     true
			"armv7-unknown-linux-gnueabihf":  true
			"armv7-unknown-linux-musleabihf": true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
			"x86_64-apple-darwin":            true
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

	configuration: {
		docker_host: {
			common: true
			description: """
				The Docker host to connect to. Use an HTTPS URL to enable TLS encryption.
				If absent, Vector will try to use `DOCKER_HOST` enviroment variable.
				If `DOCKER_HOST` is also absent, Vector will use default Docker local socket
				(`/var/run/docker.sock` on Unix flatforms, `//./pipe/docker_engine` on Windows).
				"""
			required: false
			type: string: {
				default: null
				examples: [
					"http://localhost:2375",
					"https://localhost:2376",
					"unix:///var/run/docker.sock",
					"npipe:////./pipe/docker_engine",
					"/var/run/docker.sock",
					"//./pipe/docker_engine",
				]
				syntax: "literal"
			}
		}
		tls: {
			common: false
			description: """
				TLS options to connect to the Docker deamon. This has no effect unless `docker_host` is an HTTPS URL.
				If absent, Vector will try to use environment variable `DOCKER_CERT_PATH` and then `DOCKER_CONFIG`.
				If both environment variables are absent, Vector will try to read certificates in `~/.docker/`.
				"""
			required: false
			type: object: {
				examples: []
				options: {
					ca_file: {
						description: "Path to CA certificate file."
						required:    true
						warnings: []
						type: string: {
							examples: ["certs/ca.pem"]
							syntax: "literal"
						}
					}
					crt_file: {
						description: "Path to TLS certificate file."
						required:    true
						warnings: []
						type: string: {
							examples: ["certs/cert.pem"]
							syntax: "literal"
						}
					}
					key_file: {
						description: "Path to TLS key file."
						required:    true
						warnings: []
						type: string: {
							examples: ["certs/key.pem"]
							syntax: "literal"
						}
					}
				}
			}
		}
		auto_partial_merge: {
			common: false
			description: """
				Setting this to `false` will disable the automatic merging
				of partial events.
				"""
			required: false
			type: bool: default: true
		}
		exclude_containers: {
			common: false
			description: """
				A list of container IDs _or_ names to match against for
				containers you don't want to collect logs from. Prefix matches
				are supported, so you can supply just the first few characters
				of the ID or name of containers you want to exclude. This can be
				used in conjunction with
				[`include_containers`](#include_containers).
				"""
			required: false
			type: array: {
				default: null
				items: type: string: {
					examples: ["exclude_", "exclude_me_0", "ad08cc418cf9"]
					syntax: "literal"
				}
			}
		}
		include_containers: {
			common: true
			description: """
				A list of container IDs _or_ names to match against for
				containers you want to collect logs from. Prefix matches are
				supported, so you can supply just the first few characters of
				the ID or name of containers you want to include. This can be
				used in conjunction with
				[`exclude_containers`](#exclude_containers).
				"""
			required: false
			type: array: {
				default: null
				items: type: string: {
					examples: ["include_", "include_me_0", "ad08cc418cf9"]
					syntax: "literal"
				}
			}
		}
		include_labels: {
			common:      true
			description: """
				A list of container object labels to match against when
				filtering running containers. This should follow the
				described label's syntax in [docker object labels docs](\(urls.docker_object_labels)).
				"""
			required:    false
			type: array: {
				default: null
				items: type: string: {
					examples: ["com.example.vendor=Timber Inc.", "com.example.name=Vector"]
					syntax: "literal"
				}
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
				items: type: string: {
					examples: ["httpd", "redis"]
					syntax: "literal"
				}
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
		host_key: {
			category:    "Context"
			common:      false
			description: "The key name added to each event representing the current host. This can also be globally set via the [global `host_key` option][docs.reference.configuration.global-options#host_key]."
			required:    false
			warnings: []
			type: string: {
				default: "host"
				syntax:  "literal"
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
					type: string: {
						examples: ["9b6247364a03", "715ebfcee040"]
						syntax: "literal"
					}
				}
				container_name: {
					description: "The Docker container name that the log was collected from."
					required:    true
					type: string: {
						examples: ["evil_ptolemy", "nostalgic_stallman"]
						syntax: "literal"
					}
				}
				image: {
					description: "The image name that the container is based on."
					required:    true
					type: string: {
						examples: ["ubuntu:latest", "busybox", "timberio/vector:latest-alpine"]
						syntax: "literal"
					}
				}
				message: {
					description: "The raw log message."
					required:    true
					type: string: {
						examples: ["Started GET / for 127.0.0.1 at 2012-03-10 14:28:14 +0100"]
						syntax: "literal"
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
						syntax: "literal"
					}
				}
				timestamp: {
					description: "The UTC timestamp extracted from the Docker log event."
					required:    true
					type: timestamp: {}
				}
				host: fields._local_host
				"*": {
					description: "Each container label is inserted with it's exact key/value pair."
					required:    true
					type: string: {
						examples: ["Started GET / for 127.0.0.1 at 2012-03-10 14:28:14 +0100"]
						syntax: "literal"
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
		container_metadata_fetch_errors_total: components.sources.internal_metrics.output.metrics.container_metadata_fetch_errors_total
		container_processed_events_total:      components.sources.internal_metrics.output.metrics.container_processed_events_total
		containers_unwatched_total:            components.sources.internal_metrics.output.metrics.containers_unwatched_total
		containers_watched_total:              components.sources.internal_metrics.output.metrics.containers_watched_total
		logging_driver_errors_total:           components.sources.internal_metrics.output.metrics.logging_driver_errors_total
		processed_bytes_total:                 components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:                components.sources.internal_metrics.output.metrics.processed_events_total
	}
}
