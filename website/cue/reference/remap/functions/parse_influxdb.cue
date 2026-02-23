{
  "remap": {
    "functions": {
      "parse_influxdb": {
        "anchor": "parse_influxdb",
        "name": "parse_influxdb",
        "category": "Parse",
        "description": "Parses the `value` as an [InfluxDB line protocol](https://docs.influxdata.com/influxdb/cloud/reference/syntax/line-protocol/) string, producing a list of Vector-compatible metrics.",
        "arguments": [
          {
            "name": "value",
            "description": "The string representation of the InfluxDB line protocol to parse.",
            "required": true,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "array"
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a valid InfluxDB line protocol string.",
          "field set contains a field value of type `string`.",
          "field set contains a `NaN` field value."
        ],
        "examples": [
          {
            "title": "Parse InfluxDB line protocol",
            "source": "parse_influxdb!(\"cpu,host=A,region=us-west usage_system=64i,usage_user=10u,temperature=50.5,on=true,sleep=false 1590488773254420000\")",
            "return": [
              {
                "gauge": {
                  "value": 64.0
                },
                "kind": "absolute",
                "name": "cpu_usage_system",
                "tags": {
                  "host": "A",
                  "region": "us-west"
                },
                "timestamp": "2020-05-26T10:26:13.254420Z"
              },
              {
                "gauge": {
                  "value": 10.0
                },
                "kind": "absolute",
                "name": "cpu_usage_user",
                "tags": {
                  "host": "A",
                  "region": "us-west"
                },
                "timestamp": "2020-05-26T10:26:13.254420Z"
              },
              {
                "gauge": {
                  "value": 50.5
                },
                "kind": "absolute",
                "name": "cpu_temperature",
                "tags": {
                  "host": "A",
                  "region": "us-west"
                },
                "timestamp": "2020-05-26T10:26:13.254420Z"
              },
              {
                "gauge": {
                  "value": 1.0
                },
                "kind": "absolute",
                "name": "cpu_on",
                "tags": {
                  "host": "A",
                  "region": "us-west"
                },
                "timestamp": "2020-05-26T10:26:13.254420Z"
              },
              {
                "gauge": {
                  "value": 0.0
                },
                "kind": "absolute",
                "name": "cpu_sleep",
                "tags": {
                  "host": "A",
                  "region": "us-west"
                },
                "timestamp": "2020-05-26T10:26:13.254420Z"
              }
            ]
          }
        ],
        "notices": [
          "This function will return a log event with the shape of a Vector-compatible metric,\nbut not a metric event itself. You will likely want to pipe the output of this\nfunction through a `log_to_metric` transform with the option `all_metrics` set to\n`true` to convert the metric-shaped log events to metric events so _real_ metrics\nare produced.",
          "The only metric type that is produced is a `gauge`. Each metric name is prefixed\nwith the `measurement` field, followed by an underscore (`_`), and then the\n`field key` field.",
          "`string` is the only type that is not supported as a field value, due to limitations\nof Vector's metric model."
        ],
        "pure": true
      }
    }
  }
}