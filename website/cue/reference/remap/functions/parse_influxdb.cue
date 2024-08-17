package metadata

remap: functions: parse_influxdb: {
	category: "Parse"
	description: """
		Parses the `value` as an [InfluxDB line protocol](https://docs.influxdata.com/influxdb/v1/write_protocols/line_protocol_reference/)
		string, producing a list of Vector-compatible metrics".
		"""
	notices: [
		"""
			The only metric type that is produced is a `gauge`. Each metric name is prefixed with the `measurement` field, followed
			by an underscore (`_`), and then the `field key` field.
			""",
	    """
			`string` are the type that is not supported as a field value,
			due to limitations of Vector's metric model
			""",
	]
	arguments: [
		{
			name:        "value"
			description: "The string representation of the InfluxDB line protocol to parse."
			required:    true
			type:        ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a valid InfluxDB line protocol string.",
	]
	return: types: ["array"]

	examples: [
		{
			title: "Parse InfluxDB line protocol"
			source: #"""
				parse_influxdb!("cpu,host=A,region=us-west usage_system=64i,usage_user=10u,temperature=50.5,on=true,sleep=false 1590488773254420000")
				"""#
			return: [
                        {
                            "name": "cpu_usage_system",
                            "tags": {
                                "host": "A",
                                "region": "us-west"
                            },
                            "timestamp": "2020-05-26T10:26:13.254420Z",
                            "kind": "absolute",
                            "gauge": {
                                "value": 64.0
                            }
                        },
                        {
                            "name": "cpu_usage_user",
                            "tags": {
                                "host": "A",
                                "region": "us-west"
                            },
                            "timestamp": "2020-05-26T10:26:13.254420Z",
                            "kind": "absolute",
                            "gauge": {
                                "value": 10.0
                            }
                        },
                        {
                            "name": "cpu_temperature",
                            "tags": {
                                "host": "A",
                                "region": "us-west"
                            },
                            "timestamp": "2020-05-26T10:26:13.254420Z",
                            "kind": "absolute",
                            "gauge": {
                                "value": 50.5
                            }
                        },
                        {
                            "name": "cpu_on",
                            "tags": {
                                "host": "A",
                                "region": "us-west"
                            },
                            "timestamp": "2020-05-26T10:26:13.254420Z",
                            "kind": "absolute",
                            "gauge": {
                                "value": 1.0
                            }
                        },
                        {
                            "name": "cpu_sleep",
                            "tags": {
                                "host": "A",
                                "region": "us-west"
                            },
                            "timestamp": "2020-05-26T10:26:13.254420Z",
                            "kind": "absolute",
                            "gauge": {
                                "value": 0.0
                            }
                        }
                ]
		},
	]
}
