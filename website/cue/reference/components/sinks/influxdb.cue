package metadata

components: sinks: _influxdb: {
	features: {
		send: {
			tls: enabled: false
			to: {
				service: services.influxdb

				interface: {
					socket: {
						api: {
							title: "Influx HTTP API"
							url:   urls.influxdb_http_api_v2
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "optional"
					}
				}
			}
		}
	}

	configuration: {
		bucket: {
			description: "The destination bucket for writes into InfluxDB 2."
			groups: ["v2"]
			required: true
			warnings: []
			type: string: {
				examples: ["vector-bucket", "4d2225e4d3d49f75"]
				syntax: "literal"
			}
		}
		consistency: {
			category:    "Persistence"
			common:      true
			description: "Sets the write consistency for the point for InfluxDB 1."
			groups: ["v1"]
			required: false
			warnings: []
			type: string: {
				default: null
				examples: ["any", "one", "quorum", "all"]
				syntax: "literal"
			}
		}
		database: {
			description: "Sets the target database for the write into InfluxDB 1."
			groups: ["v1"]
			required: true
			warnings: []
			type: string: {
				examples: ["vector-database", "iot-store"]
				syntax: "literal"
			}
		}
		endpoint: {
			description: "The endpoint to send data to."
			groups: ["v1", "v2"]
			required: true
			type: string: {
				examples: ["http://localhost:8086/", "https://us-west-2-1.aws.cloud1.influxdata.com", "https://us-west-2-1.aws.cloud2.influxdata.com"]
				syntax: "literal"
			}
		}
		org: {
			category:    "Auth"
			description: "Specifies the destination organization for writes into InfluxDB 2."
			groups: ["v2"]
			required: true
			warnings: []
			type: string: {
				examples: ["my-org", "33f2cff0a28e5b63"]
				syntax: "literal"
			}
		}
		password: {
			category:    "Auth"
			common:      true
			description: "Sets the password for authentication if you’ve enabled authentication for the write into InfluxDB 1."
			groups: ["v1"]
			required: false
			warnings: []
			type: string: {
				default: null
				examples: ["${INFLUXDB_PASSWORD}", "influxdb4ever"]
				syntax: "literal"
			}
		}
		retention_policy_name: {
			category:    "Persistence"
			common:      true
			description: "Sets the target retention policy for the write into InfluxDB 1."
			groups: ["v1"]
			required: false
			warnings: []
			type: string: {
				default: null
				examples: ["autogen", "one_day_only"]
				syntax: "literal"
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
				items: type: string: {
					examples: ["field1", "parent.child_field"]
					syntax: "field_path"
				}
			}
		}
		token: {
			category:    "Auth"
			description: "[Authentication token](\(urls.influxdb_authentication_token)) for InfluxDB 2."
			groups: ["v2"]
			required: true
			warnings: []
			type: string: {
				examples: ["${INFLUXDB_TOKEN}", "ef8d5de700e7989468166c40fc8a0ccd"]
				syntax: "literal"
			}
		}
		username: {
			category:    "Auth"
			common:      true
			description: "Sets the username for authentication if you’ve enabled authentication for the write into InfluxDB 1."
			groups: ["v1"]
			required: false
			warnings: []
			type: string: {
				default: null
				examples: ["todd", "vector-source"]
				syntax: "literal"
			}
		}
	}
}
