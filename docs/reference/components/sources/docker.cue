package metadata

import (
	"strings"
)

components: sources: docker: {
	title:             "Docker"
	short_description: strings.ToTitle(classes.function) + " logs through the Docker API"
	long_description: """
		[Docker][urls.docker] is an open platform for developing, shipping, and running
		applications and services. Docker enables you to separate your services from
		your infrastructure so you can ship quickly. With Docker, you can manage your
		infrastructure in the same ways you manage your services. By taking advantage
		of Docker's methodologies for shipping, testing, and deploying code quickly,
		you can significantly reduce the delay between writing code and running it in
		production.
		"""

	features: {
		checkpoint: enabled: false
		multiline: enabled:  true
		tls: enabled:        false
	}

	classes: {
		commonly_used: false
		deployment_roles: ["daemon"]
		egress_method: "stream"
		function:      "collect"
	}

	statuses: {
		delivery:    "best_effort"
		development: "beta"
	}

	support: {
		platforms: {
			docker: volumes: ["/var/run/docker.sock"]
			triples: {
				"aarch64-unknown-linux-gnu":  true
				"aarch64-unknown-linux-musl": true
				"x86_64-pc-windows-msv":      true
				"x86_64-unknown-linux-gnu":   true
				"x86_64-unknown-linux-musl":  true
				"x86_64-apple-darwin":        true
			}
		}

		requirements: [
			"""
				Docker API >= 1.24 is required.
				""",
			"""
				The [`json-file`][urls.docker_logging_driver_json_file] (default) or
				[`journald`](docker_logging_driver_journald) Docker logging driver must
				be enabled for this component to work. This is a constraint of the Docker
				API.
				""",
		]

		warnings: [
			"""
				Using Vector on Kubernetes? We highly recommend the
				[`kubernetes_logs` source](kubernetes_logs) instead.
				""",
		]
		notices: []
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
			common: true
			description: """
				A list of container object labels to match against when
				filtering running containers. This should follow the
				described label's synatx in [docker object labels docs][urls.docker_object_labels].
				"""
			required: false
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
					description: "The [standard stream][urls.standard_streams] that the log was collected from."
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
				   "stream": \(_stream),
				   "message": \(_message)
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
		connecting_to_docker: {
			title: "Connecting To The Docker Daemon"
			body: #"""
				Vector will automatically attempt to connect to the docker daemon for you. If
				the user that Vector is running under can run `docker ps` then Vector will be
				able to connect. Vector will also respect if `DOCKER_HOST` and
				`DOCKER_VERIFY_TLS` are set (as well as other Docker environment variables).
				See the [Docker daemon docs][urls.docker_daemon_socket_option].
				"""#
		}

		docker_integration_strategy: {
			title: "Docker Integration Strategy"
			body: #"""
				There are two primary ways through which you can integrate with Docker to
				receive its logs:

				1. Interact with the [Docker daemon][urls.docker_daemon] directly via the
				   `docker logs` command. (simplest)
				2. Configure a compatible [Docker logging driver][urls.docker_logging_drivers]
				   with a matching [Vector source][docs.sources]. (advanced)

				The Vector `docker` source implements option 1. This is the simplest option,
				but it is prone to performance and stability issues with _large_ deployments. If
				you experience this, please see the
				[Alternate Strategies section](#alternate-strategies) below.
				"""#
			sub_sections: [
				{
					title: "Alternate Strategies"
					body: #"""
						First, it's worth mentioning that Vector strives to guide you towards the
						optimal observability setup without presenting you with unnecessary details or
						questions. Unfortunately, there are circumstances where trade-offs must be made
						and you must determine which trade-offs are appropriate. Docker is one of these
						circumstances.

						Second, if you have a large container-based deployment you should consider using
						a platform Kubernetes. These platforms provide alternate log collection means
						that side-step the Docker logging problems. For supported platforms see Vector's
						[Platforms installation section][docs.installation.platforms].

						Finally, if you cannot use a container orchestrator then you can configure a
						compatible [Docker logging driver][urls.docker_logging_drivers] with a matching
						[Vector source][docs.sources]. For example:

						1. The [Docker `syslog` driver][urls.docker_logging_driver_syslog] with the
						   [Vector `syslog` source][docs.sources.syslog].
						2. The [Docker `journald` driver][urls.docker_logging_driver_journald] with the
						   [Vector `journald` source][docs.sources.journald].
						3. The [Docker `splunk` driver][urls.docker_logging_driver_splunk] with the
						   [Vector `splunk_hec` source][docs.sources.splunk_hec].

						To our knowledge there is no discernible difference in performance or stability
						between any of these. If we had to recommend one, we would recommend the
						`syslog` combination.
						"""#
				},
			]
		}

		docker_logging_drivers: {
			title: "Docker Logging Drivers"
			body: #"""
				In order for the Vector `docker` source to work properly, you must configure
				the [`json-file`][urls.docker_logging_driver_json_file] (default) or
				[`journald`][urls.docker_logging_driver_journald] Docker logging drivers.
				This is a requirement of the [Docker daemon][urls.docker_daemon], which Vector
				uses to integrate. See the
				[Docker Integration Strategy section](#docker-integration-strategy) for more
				info.
				"""#
		}

		message_merging: {
			title: "Merging Split Messages"
			body: #"""
				Docker, by default, will split log messages that exceed 16kb. This can be a
				rather frustrating problem because it produces malformed log messages that are
				difficult to work with. Vector's `docker` source solves this by default,
				automatically merging these messages into a single message. You can turn this
				off via the `auto_partial_merge` option. Furthermore, you can adjust the marker
				that we use to determine if an event is partial via the
				`partial_event_marker_field` option.
				"""#
		}
	}
}
