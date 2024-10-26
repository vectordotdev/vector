package metadata

remap: functions: parse_influxdb: {
	category: "Parse"
	description: """
		Parses the `value` as an [InfluxDB line protocol](https://docs.influxdata.com/influxdb/cloud/reference/syntax/line-protocol/)
		string, producing a list of Vector-compatible metrics.
		"""
	notices: [
		"""
			This function will return a log event with the shape of a Vector-compatible metric, but not a metric event itself.
			You will likely want to pipe the output of this function through a `log_to_metric` transform with the option `all_metrics`
			set to `true` to convert the metric-shaped log events to metric events so _real_ metrics are produced.
			""",
		"""
			The only metric type that is produced is a `gauge`. Each metric name is prefixed with the `measurement` field, followed
			by an underscore (`_`), and then the `field key` field.
			""",
		"""
			`string` is the only type that is not supported as a field value,
			due to limitations of Vector's metric model.
			""",
	]
	arguments: [
		{
			name:        "value"
			description: "The string representation of the InfluxDB line protocol to parse."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a valid InfluxDB line protocol string.",
		"field set contains a field value of type `string`.",
		"field set contains a `NaN` field value.",
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
					"name": "cpu_usage_system"
					"tags": {
						"host":   "A"
						"region": "us-west"
					}
					"timestamp": "2020-05-26T10:26:13.254420Z"
					"kind":      "absolute"
					"gauge": {
						"value": 64.0
					}
				},
				{
					"name": "cpu_usage_user"
					"tags": {
						"host":   "A"
						"region": "us-west"
					}
					"timestamp": "2020-05-26T10:26:13.254420Z"
					"kind":      "absolute"
					"gauge": {
						"value": 10.0
					}
				},
				{
					"name": "cpu_temperature"
					"tags": {
						"host":   "A"
						"region": "us-west"
					}
					"timestamp": "2020-05-26T10:26:13.254420Z"
					"kind":      "absolute"
					"gauge": {
						"value": 50.5
					}
				},
				{
					"name": "cpu_on"
					"tags": {
						"host":   "A"
						"region": "us-west"
					}
					"timestamp": "2020-05-26T10:26:13.254420Z"
					"kind":      "absolute"
					"gauge": {
						"value": 1.0
					}
				},
				{
					"name": "cpu_sleep"
					"tags": {
						"host":   "A"
						"region": "us-west"
					}
					"timestamp": "2020-05-26T10:26:13.254420Z"
					"kind":      "absolute"
					"gauge": {
						"value": 0.0
					}
				},
			]
		},
	]
}
