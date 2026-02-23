{
  "remap": {
    "functions": {
      "parse_aws_alb_log": {
        "anchor": "parse_aws_alb_log",
        "name": "parse_aws_alb_log",
        "category": "Parse",
        "description": "Parses `value` in the [Elastic Load Balancer Access format](https://docs.aws.amazon.com/elasticloadbalancing/latest/application/load-balancer-access-logs.html#access-log-entry-examples).",
        "arguments": [
          {
            "name": "value",
            "description": "Access log of the Application Load Balancer.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "strict_mode",
            "description": "When set to `false`, the parser ignores any newly added or trailing fields in AWS ALB logs instead of failing. Defaults to `true` to preserve strict parsing behavior.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "true"
          }
        ],
        "return": {
          "types": [
            "object"
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a properly formatted AWS ALB log."
        ],
        "examples": [
          {
            "title": "Parse AWS ALB log",
            "source": "parse_aws_alb_log!(\n    \"http 2018-11-30T22:23:00.186641Z app/my-loadbalancer/50dc6c495c0c9188 192.168.131.39:2817 - 0.000 0.001 0.000 200 200 34 366 \\\"GET http://www.example.com:80/ HTTP/1.1\\\" \\\"curl/7.46.0\\\" - - arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067 \\\"Root=1-58337364-23a8c76965a2ef7629b185e3\\\" \\\"-\\\" \\\"-\\\" 0 2018-11-30T22:22:48.364000Z \\\"forward\\\" \\\"-\\\" \\\"-\\\" \\\"-\\\" \\\"-\\\" \\\"-\\\" \\\"-\\\"\"\n)\n",
            "return": {
              "actions_executed": "forward",
              "chosen_cert_arn": null,
              "classification": null,
              "classification_reason": null,
              "client_host": "192.168.131.39:2817",
              "domain_name": null,
              "elb": "app/my-loadbalancer/50dc6c495c0c9188",
              "elb_status_code": "200",
              "error_reason": null,
              "matched_rule_priority": "0",
              "received_bytes": 34,
              "redirect_url": null,
              "request_creation_time": "2018-11-30T22:22:48.364000Z",
              "request_method": "GET",
              "request_processing_time": 0.0,
              "request_protocol": "HTTP/1.1",
              "request_url": "http://www.example.com:80/",
              "response_processing_time": 0.0,
              "sent_bytes": 366,
              "ssl_cipher": null,
              "ssl_protocol": null,
              "target_group_arn": "arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067",
              "target_host": null,
              "target_port_list": [],
              "target_processing_time": 0.001,
              "target_status_code": "200",
              "target_status_code_list": [],
              "timestamp": "2018-11-30T22:23:00.186641Z",
              "trace_id": "Root=1-58337364-23a8c76965a2ef7629b185e3",
              "traceability_id": null,
              "type": "http",
              "user_agent": "curl/7.46.0"
            }
          },
          {
            "title": "Parse AWS ALB log with trailing fields (non-strict mode)",
            "source": "parse_aws_alb_log!(\n    \"http 2018-11-30T22:23:00.186641Z app/my-loadbalancer/50dc6c495c0c9188 192.168.131.39:2817 - 0.000 0.001 0.000 200 200 34 366 \\\"GET http://www.example.com:80/ HTTP/1.1\\\" \\\"curl/7.46.0\\\" - - arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067 \\\"Root=1-58337364-23a8c76965a2ef7629b185e3\\\" \\\"-\\\" \\\"-\\\" 0 2018-11-30T22:22:48.364000Z \\\"forward\\\" \\\"-\\\" \\\"-\\\" \\\"-\\\" \\\"-\\\" \\\"-\\\" \\\"-\\\" TID_12345 \\\"-\\\" \\\"-\\\" \\\"-\\\"\",\n    strict_mode: false\n)\n",
            "return": {
              "actions_executed": "forward",
              "chosen_cert_arn": null,
              "classification": null,
              "classification_reason": null,
              "client_host": "192.168.131.39:2817",
              "domain_name": null,
              "elb": "app/my-loadbalancer/50dc6c495c0c9188",
              "elb_status_code": "200",
              "error_reason": null,
              "matched_rule_priority": "0",
              "received_bytes": 34,
              "redirect_url": null,
              "request_creation_time": "2018-11-30T22:22:48.364000Z",
              "request_method": "GET",
              "request_processing_time": 0.0,
              "request_protocol": "HTTP/1.1",
              "request_url": "http://www.example.com:80/",
              "response_processing_time": 0.0,
              "sent_bytes": 366,
              "ssl_cipher": null,
              "ssl_protocol": null,
              "target_group_arn": "arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067",
              "target_host": null,
              "target_port_list": [],
              "target_processing_time": 0.001,
              "target_status_code": "200",
              "target_status_code_list": [],
              "timestamp": "2018-11-30T22:23:00.186641Z",
              "trace_id": "Root=1-58337364-23a8c76965a2ef7629b185e3",
              "traceability_id": "TID_12345",
              "type": "http",
              "user_agent": "curl/7.46.0"
            }
          }
        ],
        "pure": true
      }
    }
  }
}