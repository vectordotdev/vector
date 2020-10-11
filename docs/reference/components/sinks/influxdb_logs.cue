package metadata

components: sinks: influxdb_logs: {
	title:             "InfluxDB Logs"
	short_description: "Batches log events to [InfluxDB][urls.influxdb] using [v1][urls.influxdb_http_api_v1] or [v2][urls.influxdb_http_api_v2] HTTP API."
	long_description:  "[InfluxDB][urls.influxdb] is an open-source time series database developed by InfluxData. It is written in Go and optimized for fast, high-availability storage and retrieval of time series data in fields such as operations monitoring, application metrics, Internet of Things sensor data, and real-time analytics."

	classes: {
		commonly_used: false
		egress_method: "batch"
		function:      "transmit"
		service_providers: ["InfluxData"]
	}

	features: {
		batch: {
			enabled:      true
			common:       false
			max_bytes:    1049000
			max_events:   null
			timeout_secs: 1
		}
		buffer: enabled:      true
		compression: enabled: false
		encoding: codec: enabled: false
		healthcheck: enabled: true
		request: {
			enabled:                    true
			in_flight_limit:            5
			rate_limit_duration_secs:   1
			rate_limit_num:             5
			retry_initial_backoff_secs: 1
			retry_max_duration_secs:    10
			timeout_secs:               60
		}
		tls: enabled: false
	}

	statuses: {
		delivery:    "at_least_once"
		development: "beta"
	}

	support: {
		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		bucket: {
			description: "The destination bucket for writes into InfluxDB 2."
			groups: ["v2"]
			required: true
			warnings: []
			type: string: {
				examples: ["vector-bucket", "4d2225e4d3d49f75"]
			}
		}
		consistency: {
			common:      true
			description: "Sets the write consistency for the point for InfluxDB 1."
			groups: ["v1"]
			required: false
			warnings: []
			type: string: {
				default: null
				examples: ["any", "one", "quorum", "all"]
			}
		}
		database: {
			description: "Sets the target database for the write into InfluxDB 1."
			groups: ["v1"]
			required: true
			warnings: []
			type: string: {
				examples: ["vector-database", "iot-store"]
			}
		}
		endpoint: {
			description: "The endpoint to send logs to."
			groups: ["v1", "v2"]
			required: true
			type: string: {
				examples: ["http://localhost:8086/", "https://us-west-2-1.aws.cloud1.influxdata.com", "https://us-west-2-1.aws.cloud2.influxdata.com"]
			}
		}
		namespace: {
			description: "A prefix that will be added to all metric names."
			groups: ["v1", "v2"]
			required: true
			warnings: []
			type: string: {
				examples: ["service"]
			}
		}
		org: {
			description: "Specifies the destination organization for writes into InfluxDB 2."
			groups: ["v2"]
			required: true
			warnings: []
			type: string: {
				examples: ["my-org", "33f2cff0a28e5b63"]
			}
		}
		password: {
			common:      true
			description: "Sets the password for authentication if you’ve enabled authentication for the write into InfluxDB 1."
			groups: ["v1"]
			required: false
			warnings: []
			type: string: {
				default: null
				examples: ["${INFLUXDB_PASSWORD}", "influxdb4ever"]
			}
		}
		retention_policy_name: {
			common:      true
			description: "Sets the target retention policy for the write into InfluxDB 1."
			groups: ["v1"]
			required: false
			warnings: []
			type: string: {
				default: null
				examples: ["autogen", "one_day_only"]
			}
		}
		tags: {
			common:      false
			description: "A set of additional fields that will be attached to each LineProtocol as a tag. Note: If the set of tag values has high cardinality this also increase cardinality in InfluxDB."
			groups: ["v1", "v2"]
			required: false
			warnings: []
			type: array: {
				default: null
				items: type: string: examples: ["field1", "parent.child_field"]
			}
		}
		token: {
			description: "[Authentication token][urls.influxdb_authentication_token] for InfluxDB 2."
			groups: ["v2"]
			required: true
			warnings: []
			type: string: {
				examples: ["${INFLUXDB_TOKEN}", "ef8d5de700e7989468166c40fc8a0ccd"]
			}
		}
		username: {
			common:      true
			description: "Sets the username for authentication if you’ve enabled authentication for the write into InfluxDB 1."
			groups: ["v1"]
			required: false
			warnings: []
			type: string: {
				default: null
				examples: ["todd", "vector-source"]
			}
		}
	}

	input: {
		logs:    true
		metrics: false
	}

	how_it_works: {
		mapping: {
			title: "Mapping Log Fields"
			body: #"""
				InfluxDB uses [line protocol][urls.influxdb_line_protocol] to write data points. It is a text-based format that provides the measurement, tag set, field set, and timestamp of a data point.

				A `Log Event` event contains an arbitrary set of fields (key/value pairs) that describe the event.

				The following matrix outlines how Log Event fields are mapped into InfluxDB Line Protocol:

				| Field         | Line Protocol     |                                                                                                                                                 |
				|---------------|-------------------|
				| host          | tag               |
				| message       | field             |
				| source_type   | tag               |
				| timestamp     | timestamp         |
				| [custom-key]  | field             |

				The default behavior can be overridden by a `tags` configuration.
				"""#
		}
	}
}
