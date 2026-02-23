{
  "remap": {
    "functions": {
      "parse_aws_cloudwatch_log_subscription_message": {
        "anchor": "parse_aws_cloudwatch_log_subscription_message",
        "name": "parse_aws_cloudwatch_log_subscription_message",
        "category": "Parse",
        "description": "Parses AWS CloudWatch Logs events (configured through AWS Cloudwatch subscriptions) from the `aws_kinesis_firehose` source.",
        "arguments": [
          {
            "name": "value",
            "description": "The string representation of the message to parse.",
            "required": true,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "object"
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a properly formatted AWS CloudWatch Log subscription message."
        ],
        "examples": [
          {
            "title": "Parse AWS Cloudwatch Log subscription message",
            "source": "parse_aws_cloudwatch_log_subscription_message!(s'{\n    \"messageType\": \"DATA_MESSAGE\",\n    \"owner\": \"111111111111\",\n    \"logGroup\": \"test\",\n    \"logStream\": \"test\",\n    \"subscriptionFilters\": [\n        \"Destination\"\n    ],\n    \"logEvents\": [\n        {\n            \"id\": \"35683658089614582423604394983260738922885519999578275840\",\n            \"timestamp\": 1600110569039,\n            \"message\": \"{\\\"bytes\\\":26780,\\\"datetime\\\":\\\"14/Sep/2020:11:45:41-0400\\\",\\\"host\\\":\\\"157.130.216.193\\\",\\\"method\\\":\\\"PUT\\\",\\\"protocol\\\":\\\"HTTP/1.0\\\",\\\"referer\\\":\\\"https://www.principalcross-platform.io/markets/ubiquitous\\\",\\\"request\\\":\\\"/expedite/convergence\\\",\\\"source_type\\\":\\\"stdin\\\",\\\"status\\\":301,\\\"user-identifier\\\":\\\"-\\\"}\"\n        }\n    ]\n}')\n",
            "return": {
              "log_events": [
                {
                  "id": "35683658089614582423604394983260738922885519999578275840",
                  "message": "{\"bytes\":26780,\"datetime\":\"14/Sep/2020:11:45:41-0400\",\"host\":\"157.130.216.193\",\"method\":\"PUT\",\"protocol\":\"HTTP/1.0\",\"referer\":\"https://www.principalcross-platform.io/markets/ubiquitous\",\"request\":\"/expedite/convergence\",\"source_type\":\"stdin\",\"status\":301,\"user-identifier\":\"-\"}",
                  "timestamp": "2020-09-14T19:09:29.039Z"
                }
              ],
              "log_group": "test",
              "log_stream": "test",
              "message_type": "DATA_MESSAGE",
              "owner": "111111111111",
              "subscription_filters": [
                "Destination"
              ]
            }
          }
        ],
        "pure": true
      }
    }
  }
}